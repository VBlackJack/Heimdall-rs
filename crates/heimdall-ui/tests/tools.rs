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

//! The Tools area drawn headless: the sidebar's "Sessions | Tools" and Ctrl+Shift+T, its
//! Tools tab and filter, the Tools page with its sections, cards and pins, and each tool
//! opened in its tab.
//!
//! Strings are the fallback language's (English): the tests never select a language.
//! Setting `HEIMDALL_SNAPSHOT_DIR` writes a PNG of each state there, for a visual pass.

mod common;

use std::path::Path;
use std::sync::Arc;

use heimdall_app::tools::{SidebarTab, ToolCategory, ToolGroup, ToolId};
use heimdall_app::{App, AppConfig, Message as AppMessage, TabId, TabProfile, ToolsMessage};
use heimdall_core::tools::diff_engine::DiffOptions;
use heimdall_core::tools::text_case_codec::TextCaseStyle;
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Destination, Message, Shell, sidebar_tab_id};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::terminal_view::keys::{WindowShortcut, window_shortcut};
use heimdall_ui::tools::{
    self, DiffMessage, JsonMessage, RegexMessage, TextCaseMessage, ToolMessage, catalog,
};
use iced::keyboard::{self, key::Physical};
use iced::widget::text_editor::{Action, Edit};
use iced::{Settings, Size};

/// Size of the simulated window, in logical pixels.
const WINDOW: Size = Size::new(1200.0, 760.0);

/// Environment variable naming a directory for PNG snapshots.
const SNAPSHOT_VARIABLE: &str = "HEIMDALL_SNAPSHOT_DIR";

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

fn settings() -> Settings {
    Settings {
        fonts: FONTS.iter().map(|face| (*face).into()).collect(),
        ..Settings::default()
    }
}

fn simulator(shell: &Shell) -> common::Drawn<'_> {
    common::simulator(settings(), WINDOW, shell.view())
}

/// Writes a PNG of the window when `HEIMDALL_SNAPSHOT_DIR` is set.
fn snapshot(shell: &Shell, name: &str) {
    let Some(dir) = std::env::var_os(SNAPSHOT_VARIABLE) else {
        return;
    };
    let path = Path::new(&dir).join(name);
    // iced names the picture after its renderer: an old one is cleared first, else it is
    // only compared against.
    let stem = name.trim_end_matches(".png");
    if let Ok(entries) = std::fs::read_dir(Path::new(&dir)) {
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file == name || file.starts_with(&format!("{stem}-")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    simulator(shell)
        .snapshot(&shell.theme())
        .expect("drawn")
        .matches_image(path)
        .expect("written");
}

fn tools(message: ToolsMessage) -> Message {
    Message::App(AppMessage::Tools(message))
}

#[test]
fn the_sidebar_switches_between_sessions_and_tools_and_has_no_local_shell_button() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    {
        let mut ui = simulator(&shell);
        assert!(ui.find("Local shell").is_err(), "the C# has no such button");
        ui.find(sidebar_tab_id(SidebarTab::Sessions))
            .expect("Sessions");
        ui.click(sidebar_tab_id(SidebarTab::Tools)).expect("Tools");
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages.iter().any(|message| matches!(
                message,
                Message::App(AppMessage::Tools(ToolsMessage::ShowSidebarTab(
                    SidebarTab::Tools
                )))
            )),
            "{messages:?}"
        );
    }
    let _ = shell.update(tools(ToolsMessage::ShowSidebarTab(SidebarTab::Tools)));
    assert_eq!(shell.app().sidebar_tab(), SidebarTab::Tools);
    let mut ui = simulator(&shell);
    ui.find("Filter tools...").expect("the Tools tab's filter");
    ui.find("Encoding & Format").expect("a category");
    ui.find("Base64 Encoder / Decoder").expect("a tool");
    ui.click(sidebar_tab_id(SidebarTab::Sessions))
        .expect("Sessions");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::App(AppMessage::Tools(ToolsMessage::ShowSidebarTab(
            SidebarTab::Sessions
        )))
    )));
}

#[test]
fn ctrl_shift_t_toggles_the_sidebar_tab_and_the_choice_is_kept() {
    let ctrl_shift = keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT;
    let t = keyboard::Key::Character("t".into());
    let physical = Physical::Code(keyboard::key::Code::KeyT);
    assert_eq!(
        window_shortcut(&t, physical, ctrl_shift),
        Some(WindowShortcut::ToggleToolsPanel)
    );
    assert_ne!(
        window_shortcut(&t, physical, keyboard::Modifiers::CTRL),
        Some(WindowShortcut::ToggleToolsPanel),
        "Ctrl+T alone is not it"
    );
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Shortcut(WindowShortcut::ToggleToolsPanel));
    assert_eq!(shell.app().sidebar_tab(), SidebarTab::Tools);
    // Kept across runs, as the C# `ShowToolsPanel`.
    assert_eq!(app(dir.path()).sidebar_tab(), SidebarTab::Tools);
    let _ = shell.update(Message::Shortcut(WindowShortcut::ToggleToolsPanel));
    assert_eq!(shell.app().sidebar_tab(), SidebarTab::Sessions);
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_tools_tab_lists_favorites_first_filters_by_name_or_alias_and_opens_a_tool() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(tools(ToolsMessage::ShowSidebarTab(SidebarTab::Tools)));
    let _ = shell.update(tools(ToolsMessage::ToggleFavorite(ToolId::Uuid)));
    let groups = catalog::sidebar_groups(shell.app(), "");
    assert_eq!(
        groups,
        [
            (ToolGroup::Favorites, vec![ToolId::Uuid]),
            (
                ToolGroup::Category(ToolCategory::Network),
                vec![
                    ToolId::ArpMon,
                    ToolId::BannerGrabber,
                    ToolId::Cert,
                    ToolId::Dns,
                    ToolId::FwTest,
                    ToolId::HttpHeaders,
                    ToolId::HttpStatus,
                    ToolId::IpConverter,
                    ToolId::NetworkCalculator,
                    ToolId::NetMap,
                    ToolId::Ping,
                    ToolId::PortScan,
                    ToolId::SnmpWalk,
                    ToolId::SubnetCalculator,
                    ToolId::TcpPing,
                    ToolId::TcpTrace,
                    ToolId::Whois,
                ],
            ),
            (
                ToolGroup::Category(ToolCategory::Security),
                vec![
                    ToolId::CertGen,
                    ToolId::CveLookup,
                    ToolId::DefaultCreds,
                    ToolId::DnsSec,
                    ToolId::Hash,
                    ToolId::Hmac,
                    ToolId::Jwt,
                    ToolId::PwdAudit,
                    ToolId::Password,
                    ToolId::SecNumCloud,
                    ToolId::Smb,
                    ToolId::SshAudit,
                    ToolId::SshKey,
                    ToolId::TlsAudit,
                    ToolId::Totp,
                ],
            ),
            (
                ToolGroup::Category(ToolCategory::Encoding),
                vec![
                    ToolId::Base64,
                    ToolId::JsonFormatter,
                    ToolId::RegexTester,
                    ToolId::TextCase,
                    ToolId::TextDiff,
                    ToolId::UrlEncoder,
                ],
            ),
            (
                ToolGroup::Category(ToolCategory::System),
                vec![
                    ToolId::Chmod,
                    ToolId::CmdLib,
                    ToolId::CronJob,
                    ToolId::Crontab,
                    ToolId::DateTime,
                    ToolId::Diagram,
                    ToolId::Hosts,
                    ToolId::LogView,
                    ToolId::Notes,
                    ToolId::Services,
                    ToolId::SshConfig,
                    ToolId::Ulid,
                    ToolId::Uuid,
                ],
            ),
            (
                ToolGroup::Category(ToolCategory::External),
                vec![
                    ToolId::DnsBatch,
                    ToolId::NetIf,
                    ToolId::OpenPorts,
                    ToolId::Routes,
                    ToolId::WakeOnLan,
                    ToolId::Wifi,
                ],
            ),
        ]
    );
    snapshot(&shell, "tools-sidebar.png");
    // An alias finds its tool; a group finding nothing is left out.
    assert_eq!(
        catalog::sidebar_groups(shell.app(), "GUID"),
        [
            (ToolGroup::Favorites, vec![ToolId::Uuid]),
            (
                ToolGroup::Category(ToolCategory::System),
                vec![ToolId::Uuid]
            ),
        ]
    );
    let _ = shell.update(Message::ToolsFilter("zzz".to_owned()));
    {
        let mut ui = simulator(&shell);
        ui.find("No matching tool").expect("said");
    }
    let _ = shell.update(Message::ToolsFilter("url".to_owned()));
    let mut ui = simulator(&shell);
    assert!(ui.find("Base64 Encoder / Decoder").is_err(), "filtered out");
    ui.click("URL Encoder / Decoder").expect("the tool");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::OpenTool(ToolId::UrlEncoder)))
    );
}

#[test]
fn a_folded_group_hides_its_tools_and_offers_its_menu_to_pin() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(tools(ToolsMessage::ShowSidebarTab(SidebarTab::Tools)));
    {
        let mut ui = simulator(&shell);
        ui.click("Network").expect("the group");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Tools(ToolsMessage::ToggleGroup(
                ToolGroup::Category(ToolCategory::Network)
            )))
        )));
    }
    let _ = shell.update(tools(ToolsMessage::ToggleGroup(ToolGroup::Category(
        ToolCategory::Network,
    ))));
    let mut ui = simulator(&shell);
    assert!(ui.find("Subnet Calculator").is_err(), "folded");
    ui.find("Certificate Generator")
        .expect("another group stays open");
}

#[test]
fn the_tools_page_shows_its_sections_and_cards_and_pins_a_tool() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Navigate(Destination::Tools));
    {
        let mut ui = simulator(&shell);
        ui.find("Favorites").expect("section");
        ui.find("Pin your favorite tools for quick access")
            .expect("no tool pinned");
        ui.find("All Tools").expect("section");
        ui.find("SECURITY").expect("category");
        ui.find("ENCODING & FORMAT").expect("category");
        ui.find("SYSTEM").expect("category");
        ui.find(format!("{} tools", ToolId::ALL.len()))
            .expect("count");
        ui.find("UUID/GUID generator with multiple format options")
            .expect("description");
        assert!(ui.find("Recently Used").is_err(), "nothing used yet");
        ui.click("Subnet Calculator").expect("card");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::OpenTool(ToolId::SubnetCalculator)))
        );
    }
    let _ = shell.update(tools(ToolsMessage::ToggleFavorite(ToolId::Base64)));
    let _ = shell.update(Message::OpenTool(ToolId::Uuid));
    let _ = shell.update(Message::Navigate(Destination::Tools));
    let sections = catalog::page_sections(shell.app(), "");
    assert_eq!(sections.favorites, Some(vec![ToolId::Base64]));
    assert_eq!(sections.recent, Some(vec![ToolId::Uuid]));
    assert_eq!(sections.count, ToolId::ALL.len());
    snapshot(&shell, "tools-page.png");
    {
        let mut ui = simulator(&shell);
        ui.find("Recently Used").expect("section");
        assert!(ui.find("Pin your favorite tools for quick access").is_err());
    }
    // The pin is kept across runs, as the C# `FavoriteToolIds`.
    assert_eq!(app(dir.path()).favorite_tools(), [ToolId::Base64]);
    // A search lists what it finds by category alone, and counts it.
    let found = catalog::page_sections(shell.app(), "percent");
    assert_eq!(found.favorites, None);
    assert_eq!(found.recent, None);
    assert_eq!(found.count, 0);
    let found = catalog::page_sections(shell.app(), "query strings");
    assert_eq!(
        found.categories,
        [(ToolCategory::Encoding, vec![ToolId::UrlEncoder])]
    );
    let _ = shell.update(Message::ToolsSearch("nothing like it".to_owned()));
    let mut ui = simulator(&shell);
    ui.find("No matching tool").expect("said");
    ui.find("0 tools").expect("count");
}

#[test]
fn a_card_pin_toggles_its_tool() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Navigate(Destination::Tools));
    let mut ui = simulator(&shell);
    // A card the window shows without scrolling: the network tools of batch 1 put the
    // Encoding section, the URL encoder clicked before, below the simulated window, where a
    // click finds nothing.
    ui.click(catalog::pin_id(ToolId::SubnetCalculator))
        .expect("pin");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Tools(ToolsMessage::ToggleFavorite(
                ToolId::SubnetCalculator
            )))
        )),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::OpenTool(_))),
        "the pin does not open the tool"
    );
}

#[test]
fn the_empty_window_offers_to_explore_the_tools() {
    let dir = tempfile::tempdir().expect("dir");
    let shell = Shell::with_app(app(dir.path()));
    let mut ui = simulator(&shell);
    ui.click("Explore Tools").expect("button");
    assert!(
        ui.into_messages()
            .any(|message| matches!(message, Message::Navigate(Destination::Tools)))
    );
}

#[test]
fn opening_a_tool_shows_its_tab_on_the_sessions_page_once() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Navigate(Destination::Tools));
    let _ = shell.update(Message::OpenTool(ToolId::Uuid));
    let _ = shell.update(Message::OpenTool(ToolId::Uuid));
    assert_eq!(shell.app().tabs.len(), 1, "one tab per tool, as the C#");
    let tab = &shell.app().tabs[0];
    assert!(matches!(tab.profile, TabProfile::Tool(ToolId::Uuid)));
    assert_eq!(tab.display_title(), tools::label(ToolId::Uuid));
    snapshot(&shell, "tools-uuid.png");
    let mut ui = simulator(&shell);
    ui.find("Generated UUID (v4)")
        .expect("the tool, on the Sessions page");
    ui.find("Batch Generation").expect("its batch");
    ui.find("Generate Batch").expect("its button");
}

/// Widest a tool's content is drawn, as the C# `ToolContentMaxWidth` (`CommonControls.xaml:75`).
const TOOL_CONTENT_WIDTH: f32 = 700.0;

/// Room at each side of a tool's header, as the C# `ToolHeaderPadding` (`12,8`).
const TOOL_HEADER_SIDE: f32 = 12.0;

/// Side of the help button, as the C# `ToolHelpButtonStyle` (`CommonControls.xaml:558`).
const TOOL_HELP_SIDE: f32 = 32.0;

/// Side of a tab's close button, as the C# `TabCloseButton` (`MainWindow.xaml:1722`).
const TAB_CLOSE_SIDE: f32 = 24.0;

/// Slack for text bounds against the box they sit in.
const LAYOUT_SLACK: f32 = 1.5;

#[test]
fn a_tool_s_content_is_centred_at_most_700_wide_as_the_c_sharp_stack_panel() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::Hash));
    let title = tools::label(ToolId::Hash);
    let mut ui = simulator(&shell);
    let help = common::last_bounds(&mut ui, "?");
    // The header's own title, level with the help button: the tab carries the name too.
    let header_title = common::all_bounds(&mut ui, &title)
        .into_iter()
        .find(|bounds| (bounds.center_y() - help.center_y()).abs() < help.height)
        .expect("the header's title");
    let input = common::last_bounds(&mut ui, "Input text");
    let pane_left = header_title.x - TOOL_HEADER_SIDE;
    // The "?" is the text inside its 32-wide button, which sits at the header's padding.
    let pane_right = help.center_x() + TOOL_HELP_SIDE / 2.0 + TOOL_HEADER_SIDE;
    let left = input.x - pane_left;
    let right = pane_right - (input.x + TOOL_CONTENT_WIDTH);
    assert!(
        (left - right).abs() < LAYOUT_SLACK,
        "centred: {left} left, {right} right"
    );
    assert!(
        left > TOOL_HEADER_SIDE * 2.0,
        "not at the page's left edge: {left}"
    );
}

#[test]
fn a_tab_s_close_button_is_24_by_24_as_the_c_sharp_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::Hash));
    let tab = shell.app().tabs[0].id;
    let mut ui = simulator(&shell);
    let bounds = ui
        .find(heimdall_ui::shell::tab_close_id(tab))
        .expect("the close button")
        .bounds();
    assert!(
        (bounds.width - TAB_CLOSE_SIDE).abs() < LAYOUT_SLACK,
        "{bounds:?}"
    );
    assert!(
        (bounds.height - TAB_CLOSE_SIDE).abs() < LAYOUT_SLACK,
        "{bounds:?}"
    );
}

#[test]
fn tab_moves_between_a_tool_s_fields_as_wpf_moves_through_its_controls() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    assert!(!shell.tab_moves_focus(), "no tab, no page: Tab is free");
    let _ = shell.update(Message::Navigate(Destination::Tools));
    let _ = shell.update(Message::OpenTool(ToolId::Hmac));
    assert!(
        matches!(shell.app().tabs[0].profile, TabProfile::Tool(ToolId::Hmac)),
        "the tool's tab has the keyboard"
    );
    assert!(
        shell.tab_moves_focus(),
        "from one of the tool's fields to the next"
    );
}

#[test]
fn the_base64_tool_encodes_from_its_button_and_shows_its_help() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::Base64));
    let tab = shell.app().tabs[0].id;
    {
        let mut ui = simulator(&shell);
        ui.find("Enter text and press Encode or Decode.")
            .expect("empty state");
        ui.find("URL-safe (RFC 4648)").expect("option");
        ui.click("Encode →").expect("button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Tool(id, ToolMessage::Base64(tools::Base64Message::Encode)) if id == tab
        )));
    }
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Base64(tools::Base64Message::Encode),
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("Encoded 0 bytes").expect("status");
        ui.click("?").expect("help button");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::Tool(_, ToolMessage::ToggleHelp)))
        );
    }
    let _ = shell.update(Message::Tool(tab, ToolMessage::ToggleHelp));
    snapshot(&shell, "tools-base64.png");
    let mut ui = simulator(&shell);
    ui.find(
        heimdall_ui::i18n::LOADER
            .get("ui-tool-base64-help")
            .as_str(),
    )
    .expect("help");
}

#[test]
fn a_failed_export_is_said_under_the_header_until_one_is_written() {
    use heimdall_ui::tools::CopySlot;
    use heimdall_ui::tools::file_export::Saved;

    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::SecNumCloud));
    let tab = shell.app().tabs[0].id;
    let exported =
        |saved| Message::Tool(tab, ToolMessage::Exported(CopySlot::SecNumCloudHtml, saved));
    let failed = "Export failed: disk full";
    let _ = shell.update(exported(Saved::Failed("disk full".to_owned())));
    simulator(&shell)
        .find(failed)
        .expect("the reason, as the C# StatusExportFailed");
    let _ = shell.update(exported(Saved::Cancelled));
    simulator(&shell)
        .find(failed)
        .expect("a cancel changes nothing");
    let _ = shell.update(exported(Saved::Written));
    assert!(
        simulator(&shell).find(failed).is_err(),
        "said no more once written"
    );
}

#[test]
fn the_url_tool_encodes_as_it_is_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::UrlEncoder));
    let mut ui = simulator(&shell);
    ui.find("Strict component encoding (encode all reserved characters)")
        .expect("option");
    ui.find("Decoded").expect("box");
    ui.find("Encoded").expect("box");
}

#[test]
fn a_tool_tab_closed_lets_its_state_go() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::Base64));
    let tab = shell.app().tabs[0].id;
    let _ = shell.update(Message::App(AppMessage::RequestCloseTab(tab)));
    assert!(shell.app().tabs.is_empty());
    // A message for it now finds nothing to change.
    let _ = shell.update(Message::Tool(tab, ToolMessage::ToggleHelp));
}

#[test]
fn a_diagram_with_changes_not_saved_asks_before_its_tab_or_the_window_closes() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::Diagram);
    let _ = shell.update(Message::App(AppMessage::RequestCloseTab(tab)));
    assert!(
        shell.app().tabs.is_empty(),
        "nothing changed: closed at once"
    );
    let (mut shell, tab) = opened(dir.path(), ToolId::Diagram);
    let _ = shell.update(Message::Tool(
        tab,
        ToolMessage::Diagram(tools::DiagramMessage::Edit(paste("mine "))),
    ));
    assert!(shell.app().tabs[0].holds_unsaved_text());
    let _ = shell.update(Message::App(AppMessage::RequestCloseTab(tab)));
    assert!(matches!(
        shell.app().dialog,
        Some(heimdall_app::Dialog::ConfirmCloseTool { .. })
    ));
    {
        let mut ui = simulator(&shell);
        ui.find("\"Untitled diagram\" has unsaved changes. Save them before closing the tab?")
            .expect("the question");
    }
    // Save: the file asked where, the tab kept until it is written.
    let _ = shell.update(tools(ToolsMessage::SaveBeforeClose));
    assert_eq!(shell.app().dialog, None);
    assert_eq!(shell.app().tabs.len(), 1);
    let _ = shell.update(Message::App(AppMessage::RequestCloseTab(tab)));
    {
        let mut ui = simulator(&shell);
        ui.find("Cancel").expect("Cancel");
        ui.click("Discard").expect("Discard");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::App(AppMessage::ConfirmDialog)))
        );
    }
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    assert_eq!(shell.app().tabs.len(), 1, "cancelled: kept");
    let _ = shell.update(Message::App(AppMessage::WindowCloseRequested));
    assert!(
        matches!(
            shell.app().dialog,
            Some(heimdall_app::Dialog::ConfirmExit { unsaved: 1, .. })
        ),
        "{:?}",
        shell.app().dialog
    );
    let _ = shell.update(Message::App(AppMessage::DismissDialog));
    let _ = shell.update(Message::App(AppMessage::RequestCloseTab(tab)));
    let _ = shell.update(Message::App(AppMessage::ConfirmDialog));
    assert!(shell.app().tabs.is_empty(), "discarded: closed");
}

#[test]
fn the_tools_are_named_as_the_csharp_registry_names_them() {
    for (tool, name) in [
        (ToolId::ArpMon, "ARP Monitor"),
        (ToolId::Cert, "Certificate Inspector"),
        (ToolId::FwTest, "Firewall Tester"),
        (ToolId::SnmpWalk, "SNMP Walker"),
        (ToolId::TcpTrace, "Traceroute"),
        (ToolId::CmdLib, "Command Library"),
        (ToolId::DefaultCreds, "Default Credential Scanner"),
    ] {
        assert_eq!(tools::label(tool), name, "{tool:?}");
    }
}

#[test]
fn every_tool_has_a_name_a_description_and_texts_in_every_language() {
    for tool in ToolId::ALL {
        assert!(!tools::label(tool).is_empty());
        assert!(!tools::description(tool).is_empty());
        assert!(tools::sidebar_matches(tool, tool.prefixes()[0]));
    }
    // The help keeps its blank lines, as the C# help text.
    let help = heimdall_ui::i18n::LOADER.get("ui-tool-base64-help");
    assert!(help.contains("\n\n"), "{help:?}");
}

#[test]
fn the_hash_tool_hashes_as_it_is_typed_checks_a_hash_and_takes_a_file_dropped() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::Hash));
    let tab = shell.app().tabs[0].id;
    let hash = |message| Message::Tool(tab, ToolMessage::Hash(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Enter text or browse a file to compute hashes.")
            .expect("empty state");
        ui.find("Drop a file here or click Browse to hash a file")
            .expect("drop zone");
        ui.click("Browse File").expect("button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Tool(id, ToolMessage::Hash(tools::HashMessage::Browse)) if id == tab
        )));
    }
    let _ = shell.update(hash(tools::HashMessage::Input(paste("abc"))));
    let _ = shell.update(hash(tools::HashMessage::Verify(
        "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD".to_owned(),
    )));
    snapshot(&shell, "tools-hash.png");
    {
        let mut ui = simulator(&shell);
        ui.find("Hash results").expect("results");
        ui.find("SHA3-256").expect("a row");
        ui.find("3 bytes").expect("length");
        ui.find("\u{2713} Match (SHA256)").expect("checked");
    }
    // A file dropped on the window is the tab's, as the C# view takes it.
    let file = dir.path().join("dropped.bin");
    std::fs::write(&file, b"abc").expect("written");
    // The drop's gathering waits on the runtime's timer.
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let _inside = runtime.enter();
    let _ = shell.update(Message::FileDropped(file));
    let _ = shell.update(Message::DropGathered(
        heimdall_ui::drop_batch::DropPlace::Main,
    ));
    let mut ui = simulator(&shell);
    ui.find("Hashing file...").expect("the file read");
    ui.find("Clear file").expect("file mode");
}

#[test]
fn the_hmac_tool_computes_as_the_key_and_message_are_typed_and_checks_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::Hmac));
    let tab = shell.app().tabs[0].id;
    let hmac = |message| Message::Tool(tab, ToolMessage::Hmac(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Enter a key and a message to compute HMAC.")
            .expect("empty state");
        ui.find("Output format").expect("format");
    }
    let _ = shell.update(hmac(tools::HmacMessage::Key("Jefe".to_owned())));
    let _ = shell.update(hmac(tools::HmacMessage::Input(paste(
        "what do ya want for nothing?",
    ))));
    let _ = shell.update(hmac(tools::HmacMessage::Verify(
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843".to_owned(),
    )));
    snapshot(&shell, "tools-hmac.png");
    let mut ui = simulator(&shell);
    ui.find("32 bytes (256 bits)").expect("length");
    ui.find("Match").expect("checked");
    assert!(ui.find("Jefe").is_err(), "the key is hidden");
}

#[test]
fn the_jwt_tool_shows_a_token_pasted_and_checks_its_signature() {
    let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
        eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
        SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::Jwt));
    let tab = shell.app().tabs[0].id;
    let jwt = |message| Message::Tool(tab, ToolMessage::Jwt(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Paste a JWT token to decode its header, payload, and signature.")
            .expect("empty state");
    }
    let _ = shell.update(jwt(tools::JwtMessage::Input(paste(token))));
    let _ = shell.update(jwt(tools::JwtMessage::Secret(
        "your-256-bit-secret".to_owned(),
    )));
    {
        let mut ui = simulator(&shell);
        ui.find("No expiration claim (exp) found").expect("expiry");
        ui.find("Signature Verification").expect("card");
        ui.find("HMAC Secret").expect("its secret");
    }
    let _ = shell.update(jwt(tools::JwtMessage::Verify));
    snapshot(&shell, "tools-jwt.png");
    let mut ui = simulator(&shell);
    ui.find("\u{2714} Signature is valid").expect("checked");
    ui.find("Header").expect("part");
    ui.find("Payload").expect("part");
}

#[test]
fn the_totp_tool_refuses_a_bad_secret_and_shows_the_code_of_a_good_one() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::OpenTool(ToolId::Totp));
    let tab = shell.app().tabs[0].id;
    let totp = |message| Message::Tool(tab, ToolMessage::Totp(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Secret Key (Base32)").expect("label");
        ui.click("Start").expect("button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Tool(id, ToolMessage::Totp(tools::TotpMessage::Start)) if id == tab
        )));
    }
    let _ = shell.update(totp(tools::TotpMessage::Start));
    {
        let mut ui = simulator(&shell);
        ui.find("Please enter a secret key.").expect("refused");
    }
    let _ = shell.update(totp(tools::TotpMessage::Secret("ABC1".to_owned())));
    let _ = shell.update(totp(tools::TotpMessage::Start));
    {
        let mut ui = simulator(&shell);
        ui.find("Invalid Base32 encoding. Use characters A-Z and 2-7 only.")
            .expect("refused");
    }
    let _ = shell.update(totp(tools::TotpMessage::Secret(
        "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_owned(),
    )));
    let _ = shell.update(totp(tools::TotpMessage::Start));
    snapshot(&shell, "tools-totp.png");
    let mut ui = simulator(&shell);
    ui.find("Current Code").expect("the code's card");
    assert!(
        ui.find("Invalid Base32 encoding. Use characters A-Z and 2-7 only.")
            .is_err()
    );
    ui.click("Copy").expect("button");
    assert!(ui.into_messages().any(|message| matches!(
        message,
        Message::Tool(id, ToolMessage::Totp(tools::TotpMessage::Copy)) if id == tab
    )));
}

/// `text` pasted in a box.
fn paste(text: &str) -> Action {
    Action::Edit(Edit::Paste(Arc::new(text.to_owned())))
}

/// `tool` opened in a new window, and its tab.
fn opened(dir: &Path, tool: ToolId) -> (Shell, TabId) {
    let mut shell = Shell::with_app(app(dir));
    let _ = shell.update(Message::OpenTool(tool));
    let tab = shell.app().tabs[0].id;
    (shell, tab)
}

#[test]
fn the_json_formatter_prettifies_from_its_button_and_says_the_length() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::JsonFormatter);
    let send = |message| Message::Tool(tab, ToolMessage::Json(message));
    let _ = shell.update(send(JsonMessage::Input(paste("{\"a\":[1,2]}"))));
    {
        let mut ui = simulator(&shell);
        ui.find("Paste JSON and press Prettify or Minify.")
            .expect("empty state");
        ui.find("Input JSON").expect("label");
        ui.click("Prettify").expect("button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Tool(id, ToolMessage::Json(JsonMessage::Prettify)) if id == tab
        )));
    }
    let _ = shell.update(send(JsonMessage::Prettify));
    // Five line breaks, of two characters on Windows as .NET writes them there.
    let count = if cfg!(windows) { 34 } else { 29 };
    {
        let mut ui = simulator(&shell);
        ui.find(format!("Prettified ({count} characters)"))
            .expect("status");
        ui.find("Copy output").expect("copy");
    }
    snapshot(&shell, "tools-json.png");
    let _ = shell.update(send(JsonMessage::Minify));
    let mut ui = simulator(&shell);
    ui.find("Minified (11 characters)").expect("status");
}

#[test]
fn the_regex_tester_lists_and_counts_the_matches_after_the_pause() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::RegexTester);
    let send = |message| Message::Tool(tab, ToolMessage::Regex(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Enter a regex pattern and test string above")
            .expect("empty state");
        ui.find("Singleline").expect("option");
    }
    let _ = shell.update(send(RegexMessage::Pattern("(?<word>b+)(c)?".to_owned())));
    let _ = shell.update(send(RegexMessage::Test(paste("abbc ab"))));
    // The second change's wait over: the pattern tried.
    let _ = shell.update(send(RegexMessage::Run(2)));
    {
        let mut ui = simulator(&shell);
        ui.find("2 matches").expect("count");
        ui.find("Valid regex").expect("status");
        // .NET's numbers: the unnamed group first.
        ui.find("[0] Index 1: \"bbc\"  Group 1: \"c\"  Group 2: \"bb\"")
            .expect("first match");
        ui.find("Copy matches").expect("copy");
    }
    snapshot(&shell, "tools-regex.png");
    // A look-behind and a backreference run, as in .NET.
    let _ = shell.update(send(RegexMessage::Pattern(r"(?<=a)(b)\1".to_owned())));
    let _ = shell.update(send(RegexMessage::Run(3)));
    {
        let mut ui = simulator(&shell);
        ui.find("1 match").expect("count");
        ui.find("[0] Index 1: \"bb\"  Group 1: \"b\"")
            .expect("the match");
    }
    let _ = shell.update(send(RegexMessage::Pattern("(?<=a+)b".to_owned())));
    let _ = shell.update(send(RegexMessage::Run(4)));
    let mut ui = simulator(&shell);
    ui.find("Invalid regex: a look-behind whose length varies is not supported by this engine")
        .expect("the construct named");
    assert!(ui.find("1 match").is_err(), "the matches cleared");
}

#[test]
fn the_text_case_converter_converts_from_its_buttons() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::TextCase);
    let send = |message| Message::Tool(tab, ToolMessage::TextCase(message));
    let _ = shell.update(send(TextCaseMessage::Input(paste("hello world"))));
    {
        let mut ui = simulator(&shell);
        ui.find("Enter text and select a case conversion.")
            .expect("empty state");
        for label in [
            "camelCase",
            "PascalCase",
            "snake_case",
            "kebab-case",
            "UPPER CASE",
            "lower case",
            "Title Case",
            "CONSTANT_CASE",
        ] {
            ui.find(label).expect("a style's button");
        }
        ui.click("snake_case").expect("button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Tool(
                _,
                ToolMessage::TextCase(TextCaseMessage::Convert(TextCaseStyle::Snake))
            )
        )));
    }
    let _ = shell.update(send(TextCaseMessage::Convert(TextCaseStyle::Snake)));
    snapshot(&shell, "tools-textcase.png");
    let mut ui = simulator(&shell);
    assert!(
        ui.find("Enter text and select a case conversion.").is_err(),
        "the output shown"
    );
    ui.find("Copy").expect("copy");
}

#[test]
fn the_text_diff_compares_and_shows_its_counts_in_the_header() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::TextDiff);
    let send = |message| Message::Tool(tab, ToolMessage::Diff(message));
    let original = "alpha\nbeta gamma\ndelta";
    let modified = "alpha\nbeta omega\ndelta\nepsilon";
    let _ = shell.update(send(DiffMessage::Original(paste(original))));
    let _ = shell.update(send(DiffMessage::Modified(paste(modified))));
    {
        let mut ui = simulator(&shell);
        ui.find("Enter original and modified text, then press Compare.")
            .expect("empty state");
        ui.find("Auto-compare").expect("option");
        ui.find("Swap").expect("header button");
        ui.click("Compare").expect("button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Tool(_, ToolMessage::Diff(DiffMessage::Compare))
        )));
    }
    let _ = shell.update(send(DiffMessage::Compare));
    {
        let mut ui = simulator(&shell);
        ui.find("Comparing...").expect("busy");
    }
    // The comparison its tab runs away from the window, made here.
    let computed = tools::compute_diff(original, modified, DiffOptions::default());
    let _ = shell.update(send(DiffMessage::Computed(Some(Box::new(computed)))));
    {
        let mut ui = simulator(&shell);
        ui.find("+2 additions, -1 deletions, 2 unchanged")
            .expect("counts");
        ui.find("Diff complete: 5 lines").expect("status");
        ui.find("Copy diff").expect("copy");
    }
    snapshot(&shell, "tools-diff.png");
}

#[test]
fn a_card_is_as_tall_as_the_csharp_one_with_its_description_on_one_line() {
    let dir = tempfile::tempdir().expect("dir");
    let mut shell = Shell::with_app(app(dir.path()));
    let _ = shell.update(Message::Navigate(Destination::Tools));
    let mut ui = simulator(&shell);
    for tool in ToolId::ALL {
        let card = ui.find(catalog::card_id(tool)).expect("card").bounds();
        assert!(
            (card.width - catalog::CARD_WIDTH).abs() < 0.5,
            "{tool:?}: {card:?}"
        );
        assert!(
            (card.height - catalog::CARD_HEIGHT).abs() < 0.5,
            "{tool:?}: {card:?}"
        );
        let description = ui
            .find(tools::description(tool))
            .expect("description")
            .bounds();
        assert!(
            description.x + description.width <= card.x + card.width,
            "{tool:?}: {description:?} past {card:?}"
        );
        assert!(
            description.y + description.height <= card.y + card.height,
            "{tool:?}: {description:?} under {card:?}"
        );
    }
}

#[test]
fn the_password_generator_opens_on_a_password_and_switches_mode() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::Password);
    let send = |message| Message::Tool(tab, ToolMessage::Password(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Password Generator").expect("title");
        ui.find("Recent passwords").expect("history");
        ui.find("Uppercase (A-Z)").expect("a class");
        ui.find("Random characters from selected character sets")
            .expect("the mode's description");
        ui.click("Generate").expect("button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Tool(id, ToolMessage::Password(tools::PasswordMessage::Generate)) if id == tab
        )));
    }
    let _ = shell.update(send(tools::PasswordMessage::Set(
        heimdall_core::tools::password_generator::Setting::Mode(
            heimdall_core::tools::password_generator::GeneratorMode::Syllable,
        ),
    )));
    let _ = shell.update(send(tools::PasswordMessage::Set(
        heimdall_core::tools::password_generator::Setting::SyllableCvc(true),
    )));
    {
        let mut ui = simulator(&shell);
        ui.find("Pronounceable consonant-vowel pairs (easy to remember)")
            .expect("the mode's description");
        ui.find("Closed syllables (CVC)").expect("its option");
        ui.find("Structure:").expect("the syllables shown");
    }
    snapshot(&shell, "tools-password.png");
    let _ = shell.update(send(tools::PasswordMessage::ToggleAdvanced));
    let mut ui = simulator(&shell);
    ui.find("Remember my settings").expect("an advanced option");
}

#[test]
fn the_ssh_key_generator_shows_a_pair_its_private_key_masked() {
    use heimdall_core::tools::ssh_key_generator::{self, SshKeyAlgorithm};
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::SshKey);
    let send = |message| Message::Tool(tab, ToolMessage::SshKey(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Algorithm").expect("label");
        ui.find("Optional. Encrypts the private key with AES-256-CBC.")
            .expect("hint");
        assert!(ui.find("Fingerprint").is_err(), "nothing made yet");
        ui.click("Generate").expect("button");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::Tool(id, ToolMessage::SshKey(tools::SshKeyMessage::Generate)) if id == tab
        )));
    }
    let _ = shell.update(send(tools::SshKeyMessage::Algorithm(
        tools::AlgorithmChoice(SshKeyAlgorithm::Ed25519),
    )));
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let _inside = runtime.enter();
    let _ = shell.update(send(tools::SshKeyMessage::Generate));
    {
        let mut ui = simulator(&shell);
        ui.find("Generating...").expect("busy");
    }
    // The pair its tab makes away from the window, made here.
    let key = ssh_key_generator::generate(SshKeyAlgorithm::Ed25519, "me@host", "").expect("made");
    let _ = shell.update(send(tools::SshKeyMessage::Generated(
        1,
        Some(Ok(std::sync::Arc::new(key))),
    )));
    snapshot(&shell, "tools-sshkey.png");
    let mut ui = simulator(&shell);
    ui.find("Fingerprint").expect("fingerprint");
    ui.find("Public Key").expect("public key");
    ui.find("Private Key").expect("private key");
    ui.find("Show").expect("the key masked");
    assert!(ui.find("Generating...").is_err());
}

#[test]
fn the_certificate_generator_refuses_no_name_and_shows_what_it_made() {
    use heimdall_core::tools::certificate_generator::{self, CertificateOptions};
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::CertGen);
    let send = |message| Message::Tool(tab, ToolMessage::CertGen(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Common Name (CN)").expect("label");
        ui.find("Certificate Type").expect("type");
        ui.find("Key Size").expect("key size");
    }
    let _ = shell.update(send(tools::CertGenMessage::Generate));
    {
        let mut ui = simulator(&shell);
        ui.find("Please enter a Common Name (CN).")
            .expect("refused");
    }
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let _inside = runtime.enter();
    let _ = shell.update(send(tools::CertGenMessage::Cn("server.local".to_owned())));
    let _ = shell.update(send(tools::CertGenMessage::Generate));
    {
        let mut ui = simulator(&shell);
        ui.find("Generating...").expect("busy");
    }
    let options = CertificateOptions {
        cn: "server.local".to_owned(),
        org: String::new(),
        country: String::new(),
        key_bits: 2048,
        validity_days: 365,
        sans: Vec::new(),
    };
    let made = certificate_generator::generate_self_signed(&options, 1_767_323_045).expect("made");
    let _ = shell.update(send(tools::CertGenMessage::Generated(
        1,
        Some(Ok(std::sync::Arc::new(
            tools::GeneratedCertificates::SelfSigned(made),
        ))),
    )));
    snapshot(&shell, "tools-certgen.png");
    let mut ui = simulator(&shell);
    ui.find("Certificate (PEM)").expect("certificate");
    ui.find("Private Key (PEM)").expect("key");
    ui.find("Save .pfx").expect("export");
    drop(ui);
    let _ = shell.update(send(tools::CertGenMessage::SavePfx));
    let mut ui = simulator(&shell);
    ui.find("Enter a password for the PFX file (can be empty):")
        .expect("its password asked first");
}

#[test]
fn the_password_audit_scores_a_password_as_it_is_typed() {
    let dir = tempfile::tempdir().expect("dir");
    let (mut shell, tab) = opened(dir.path(), ToolId::PwdAudit);
    let send = |message| Message::Tool(tab, ToolMessage::PwdAudit(message));
    {
        let mut ui = simulator(&shell);
        ui.find("Enter a password to analyze its strength")
            .expect("empty state");
        ui.find("Policy").expect("policy");
    }
    let _ = shell.update(send(tools::PwdAuditMessage::Password(
        "password".to_owned(),
    )));
    snapshot(&shell, "tools-pwdaudit.png");
    let mut ui = simulator(&shell);
    ui.find("15/100 - Weak").expect("score");
    ui.find("Found in common password list").expect("common");
    ui.find("8 characters (minimum: 8)").expect("length");
    assert!(ui.find("password").is_err(), "the password is hidden");
}

#[test]
fn a_tool_opens_on_the_selected_host_or_the_palette_argument_named_after_it() {
    use heimdall_app::tools::ToolContext;
    use heimdall_core::profile::{ProfileId, SshProfile};
    use heimdall_ui::shell::TreeShortcut;

    let dir = tempfile::tempdir().expect("dir");
    let mut store =
        heimdall_core::store::ProfileStore::open(dir.path().join("profiles.toml")).expect("store");
    store.merge([SshProfile {
        id: ProfileId::new("web"),
        name: "web".to_owned(),
        group: None,
        host: "web.lab".to_owned(),
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
        remote_os: heimdall_core::profile::RemoteOs::Auto,
    }]);
    store.save().expect("save");
    let mut app = app(dir.path());
    app.selected_profile = Some(ProfileId::new("web"));
    let mut shell = Shell::with_app(app);
    let ping = tools::label(ToolId::Ping);

    // From the sidebar, a network tool on the selected session's host, named after it, as
    // the C# `ResolveToolTabTitle`; a tool reaching no host on nothing.
    let _ = shell.update(Message::OpenTool(ToolId::Ping));
    let _ = shell.update(Message::OpenTool(ToolId::Base64));
    let tabs = &shell.app().tabs;
    assert_eq!(tabs[0].display_title(), format!("{ping}: web.lab"));
    assert_eq!(tabs[0].tool_context(), &ToolContext::on_host("web.lab"));
    assert_eq!(tabs[1].display_title(), tools::label(ToolId::Base64));
    assert!(tabs[1].tool_context().is_empty());

    // From the palette, "ping 10.0.0.1": that tool alone, opened on it.
    let _ = shell.update(Message::TreeShortcut(TreeShortcut::QuickConnect));
    let _ = shell.update(Message::PaletteQuery("ping 10.0.0.1".to_owned()));
    simulator(&shell)
        .find(format!("{ping}: 10.0.0.1").as_str())
        .expect("its line");
    let _ = shell.update(Message::DialogKey { confirm: true });
    let tabs = &shell.app().tabs;
    assert_eq!(tabs.len(), 3);
    assert_eq!(tabs[2].display_title(), format!("{ping}: 10.0.0.1"));
    assert_eq!(
        tabs[2].tool_context(),
        &ToolContext::with_argument("10.0.0.1")
    );
}
