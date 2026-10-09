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
//! Tools tab and filter, the Tools page with its sections, cards and pins, and a tool opened
//! in its tab.
//!
//! Strings are the fallback language's (English): the tests never select a language.
//! Setting `HEIMDALL_SNAPSHOT_DIR` writes a PNG of each state there, for a visual pass.

mod common;

use std::path::Path;

use heimdall_app::tools::{SidebarTab, ToolCategory, ToolGroup, ToolId};
use heimdall_app::{App, AppConfig, Message as AppMessage, TabProfile, ToolsMessage};
use heimdall_ssh::AgentSource;
use heimdall_term::GridSize;
use heimdall_ui::shell::{Destination, Message, Shell, sidebar_tab_id};
use heimdall_ui::terminal_view::FONTS;
use heimdall_ui::terminal_view::keys::{WindowShortcut, window_shortcut};
use heimdall_ui::tools::{self, ToolMessage, catalog};
use iced::keyboard::{self, key::Physical};
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
                    ToolId::IpConverter,
                    ToolId::NetworkCalculator,
                    ToolId::SubnetCalculator
                ]
            ),
            (
                ToolGroup::Category(ToolCategory::Encoding),
                vec![ToolId::Base64, ToolId::UrlEncoder]
            ),
            (
                ToolGroup::Category(ToolCategory::System),
                vec![
                    ToolId::Chmod,
                    ToolId::Crontab,
                    ToolId::DateTime,
                    ToolId::SshConfig,
                    ToolId::Ulid,
                    ToolId::Uuid
                ]
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
        ui.click("System").expect("the group");
        assert!(ui.into_messages().any(|message| matches!(
            message,
            Message::App(AppMessage::Tools(ToolsMessage::ToggleGroup(
                ToolGroup::Category(ToolCategory::System)
            )))
        )));
    }
    let _ = shell.update(tools(ToolsMessage::ToggleGroup(ToolGroup::Category(
        ToolCategory::System,
    ))));
    let mut ui = simulator(&shell);
    assert!(ui.find("UUID Generator").is_err(), "folded");
    ui.find("Base64 Encoder / Decoder")
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
        ui.find("ENCODING & FORMAT").expect("category");
        ui.find("SYSTEM").expect("category");
        ui.find("11 tools").expect("count");
        ui.find("UUID/GUID generator with multiple format options")
            .expect("description");
        assert!(ui.find("Recently Used").is_err(), "nothing used yet");
        ui.click("Base64 Encoder / Decoder").expect("card");
        assert!(
            ui.into_messages()
                .any(|message| matches!(message, Message::OpenTool(ToolId::Base64)))
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
    ui.click(catalog::pin_id(ToolId::UrlEncoder)).expect("pin");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            Message::App(AppMessage::Tools(ToolsMessage::ToggleFavorite(
                ToolId::UrlEncoder
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
