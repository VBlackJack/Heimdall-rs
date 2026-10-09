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

//! The Tools area: a tool opened in a tab of its own, as the C# `OpenToolTabAsync`
//! (`MainViewModel.cs:1236-1288`); the tools pinned and used lately; the sidebar's
//! "Sessions | Tools" tab and its folded categories, as the C# `SidebarViewModel`.

use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect, Phase, Tab, TabProfile};
use crate::driver::Purpose;
use crate::ids::{AttemptId, TabId};
use crate::tools::{SidebarTab, ToolGroup, ToolId, toggle_favorite};

/// What the Tools area is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolsMessage {
    /// Open `tool` in a tab, named `title`, as the C# names it in the language shown; a
    /// tool reaching no host that is open already is shown instead, as the C#.
    Open {
        /// The tool.
        tool: ToolId,
        /// The tab's title.
        title: String,
    },
    /// Pin `tool`, or unpin it, saved at once.
    ToggleFavorite(ToolId),
    /// Show this tab of the sidebar.
    ShowSidebarTab(SidebarTab),
    /// Show the sidebar's other tab, as the C# Ctrl+Shift+T.
    ToggleSidebarTab,
    /// Fold or unfold a group of the sidebar's tools.
    ToggleGroup(ToolGroup),
}

impl Tab {
    /// The tool it shows, for a tool's tab.
    #[must_use]
    pub fn tool(&self) -> Option<ToolId> {
        match self.profile {
            TabProfile::Tool(tool) => Some(tool),
            _ => None,
        }
    }
}

impl App {
    /// The tab the sidebar shows, as the settings keep it.
    #[must_use]
    pub fn sidebar_tab(&self) -> SidebarTab {
        if self.settings.tools.show_tools_panel {
            SidebarTab::Tools
        } else {
            SidebarTab::Sessions
        }
    }

    /// The tools pinned that this version has, in the order they were pinned, each once.
    #[must_use]
    pub fn favorite_tools(&self) -> Vec<ToolId> {
        let mut tools: Vec<ToolId> = Vec::new();
        for tool in self
            .settings
            .tools
            .favorites
            .iter()
            .filter_map(|code| ToolId::from_code(code))
        {
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        }
        tools
    }

    /// Whether `tool` is pinned.
    #[must_use]
    pub fn is_favorite_tool(&self, tool: ToolId) -> bool {
        self.settings
            .tools
            .favorites
            .iter()
            .any(|code| code.eq_ignore_ascii_case(tool.code()))
    }

    /// Whether `tab` shows a tool.
    pub(super) fn is_tool(&self, tab: TabId) -> bool {
        self.tab(tab).is_some_and(|found| found.tool().is_some())
    }

    /// The tools used lately, the newest first.
    #[must_use]
    pub fn recent_tools(&self) -> &[ToolId] {
        self.recent_tools.ids()
    }

    /// Whether `group` of the sidebar's tools is unfolded: unless folded by the user, as the
    /// C# default.
    #[must_use]
    pub fn tool_group_open(&self, group: ToolGroup) -> bool {
        !self
            .settings
            .tools
            .collapsed_categories
            .iter()
            .any(|key| key == group.key())
    }

    /// The host a network tool opened now inherits: the selected session's, trimmed, as the
    /// C# `GetInheritedToolTargetHost` (`ToolsTabPopulationService.cs:709-713`); `None` when
    /// none is selected or it has no host.
    #[must_use]
    pub fn tool_target_host(&self) -> Option<String> {
        let profile = self.profile_summary(self.selected_profile.as_ref()?)?;
        let (host, _) = profile.endpoint?;
        let host = host.trim();
        (!host.is_empty()).then(|| host.to_owned())
    }

    /// Applies a message about the Tools area.
    pub(super) fn tools_message(&mut self, message: ToolsMessage) -> Vec<Effect> {
        match message {
            ToolsMessage::Open { tool, title } => self.open_tool(tool, title),
            ToolsMessage::ToggleFavorite(tool) => {
                let toggle = toggle_favorite(&self.settings.tools.favorites, tool);
                self.save_tools(|tools| tools.favorites.clone_from(&toggle.favorites));
                Vec::new()
            }
            ToolsMessage::ShowSidebarTab(tab) => {
                self.save_tools(|tools| tools.show_tools_panel = tab == SidebarTab::Tools);
                Vec::new()
            }
            ToolsMessage::ToggleSidebarTab => {
                let tab = self.sidebar_tab().other();
                self.save_tools(|tools| tools.show_tools_panel = tab == SidebarTab::Tools);
                Vec::new()
            }
            ToolsMessage::ToggleGroup(group) => {
                let key = group.key();
                let open = self.tool_group_open(group);
                self.save_tools(|tools| {
                    tools.collapsed_categories.retain(|known| known != key);
                    if open {
                        tools.collapsed_categories.push(key.to_owned());
                    }
                });
                Vec::new()
            }
        }
    }

    /// Changes the Tools area's settings with `change`, saved at once as the C#
    /// `MergeSettingAsync`; a change that cannot be saved is said and not made, as the C#
    /// favourites are persisted before anything in memory changes.
    fn save_tools(&mut self, change: impl FnOnce(&mut heimdall_core::settings::ToolsSettings)) {
        let before = self.settings.tools.clone();
        change(&mut self.settings.tools);
        if self.settings.tools == before {
            return;
        }
        if let Err(error) = self.settings.save(&self.settings_file) {
            self.settings.tools = before;
            self.dialog = Some(Dialog::save_failed(&error));
        }
    }

    /// Opens `tool` in a tab named `title`, the last, and shows it; a tool reaching no host
    /// already open is shown instead, as the C# singleton tools. Either way it becomes the
    /// tool used last, as the C# tracks it once opened.
    fn open_tool(&mut self, tool: ToolId, title: String) -> Vec<Effect> {
        let open = (!tool.is_network())
            .then(|| self.tabs.iter().find(|tab| tab.tool() == Some(tool)))
            .flatten()
            .map(|tab| tab.id);
        let effects = if let Some(id) = open {
            self.select_tab(id)
        } else {
            let id = TabId::fresh();
            let mut tab = Tab::new(
                self.terminal_palette(),
                id,
                TabProfile::Tool(tool),
                Purpose::Tool,
                self.viewport,
                AttemptId::fresh(),
                CancellationToken::new(),
            );
            tab.title = title;
            // Ready at once: nothing connects, as the C# tool tab's "Ready".
            tab.phase = Phase::Connected;
            self.tabs.push(tab);
            self.select_tab(id)
        };
        self.recent_tools.track(tool);
        effects
    }
}
