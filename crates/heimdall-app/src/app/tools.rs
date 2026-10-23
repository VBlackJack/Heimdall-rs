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

use heimdall_core::profile::{ProfileId, SshGateway};
use heimdall_ssh::HopSecrets;
use tokio_util::sync::CancellationToken;

use super::route_test::endpoint;
use super::tool_entry::ToolEntryEdit;
use super::{App, Dialog, Effect, Phase, Tab, TabProfile};
use crate::driver::Purpose;
use crate::ids::{AttemptId, TabId};
use crate::tool_route::{GatewayChoice, ToolRoute};
use crate::tools::{SidebarTab, ToolContext, ToolGroup, ToolId, toggle_favorite};

/// What the Tools area is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolsMessage {
    /// Open `tool` in a tab, named `title`, as the C# names it in the language shown, on
    /// `context`; a tool reaching no host that is open already is shown instead when
    /// `context` is empty, as the C#.
    Open {
        /// The tool.
        tool: ToolId,
        /// The tab's title.
        title: String,
        /// What it opens on, as the C# `ToolContext`.
        context: ToolContext,
    },
    /// Open the picker of the C# "Add > Tool", the entry to be filed in `folder`.
    NewEntry {
        /// The folder the entry goes in; none for the top.
        folder: Option<String>,
    },
    /// Something changed in the picker.
    EditEntry(ToolEntryEdit),
    /// Open the tool of tree entry `id` on the host and port it holds.
    OpenEntry(ProfileId),
    /// Pin `tool`, or unpin it, saved at once.
    ToggleFavorite(ToolId),
    /// Show this tab of the sidebar.
    ShowSidebarTab(SidebarTab),
    /// Show the sidebar's other tab, as the C# Ctrl+Shift+T.
    ToggleSidebarTab,
    /// Fold or unfold a group of the sidebar's tools.
    ToggleGroup(ToolGroup),
    /// Write `command` into terminal tab `target`, once the user agrees to the question it
    /// opens; `dangerous` for a command marked so. Nothing is written by this message.
    SendCommand {
        /// The terminal tab.
        target: TabId,
        /// The command, one line.
        command: String,
        /// Marked dangerous.
        dangerous: bool,
    },
    /// Type `command` at the prompt of terminal tab `target`, without Enter and without a
    /// question: nothing runs until the user presses Enter themselves, as the C# Insert.
    InsertCommand {
        /// The terminal tab.
        target: TabId,
        /// The command, one line.
        command: String,
    },
    /// Write `command` into each terminal tab of `targets`, once the user agrees to the one
    /// question it opens, which lists them all; `dangerous` for a command marked so.
    /// Nothing is written by this message.
    BroadcastCommand {
        /// The terminal tabs.
        targets: Vec<TabId>,
        /// The command, one line.
        command: String,
        /// Marked dangerous.
        dangerous: bool,
    },
    /// The close question of a tool's tab answered Save: its document saved, then the
    /// close asked again.
    SaveBeforeClose,
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

    /// Tool tab `tab` holds the document `name` with changes not saved, or nothing to lose
    /// (`None`): the window says it after each change, so closing the tab asks first.
    pub fn set_tool_unsaved(&mut self, tab: TabId, name: Option<String>) {
        if let Some(found) = self.tab_mut(tab).filter(|found| found.tool().is_some()) {
            found.tool_unsaved = name;
        }
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

    /// What the command library opened now starts with, as the C#
    /// `EmbeddedSessionManager.WithSessionProfile` fills a tool opened from a session: the
    /// host, the user and the platform of the session shown when it is one of a profile, else
    /// of the profile selected in the tree. `None` when there is neither, or it gives nothing.
    #[must_use]
    pub fn session_context(&self) -> Option<ToolContext> {
        let from_tab = self
            .shown_tab()
            .filter(|tab| tab.tool().is_none())
            .map(|tab| {
                let profile = &tab.profile;
                ToolContext {
                    session_tab: Some(tab.id),
                    ..ToolContext::of_session(
                        profile.endpoint().map(|(host, _)| host),
                        profile.username(),
                        profile.platform(),
                    )
                }
            })
            .filter(ToolContext::has_session);
        if from_tab.is_some() {
            return from_tab;
        }
        let id = self.selected_profile.as_ref()?;
        let summary = self.profile_summary(id)?;
        let platform = self
            .store
            .remote_os_of(id)
            .or(summary.kind.platform_guess());
        Some(ToolContext::of_session(
            summary.endpoint.as_ref().map(|(host, _)| host.as_str()),
            summary.username.as_deref(),
            platform,
        ))
        .filter(ToolContext::has_session)
    }

    /// The gateways a network tool can be routed through, as the C# "Route via" lists them.
    #[must_use]
    pub fn tool_gateways(&self) -> Vec<GatewayChoice> {
        self.gateways().iter().map(GatewayChoice::of).collect()
    }

    /// The route to gateway `id` as it is now, each hop with the secrets saved for it and for
    /// its endpoint only; `None` when the gateway is gone or its route is not one (a loop, too
    /// deep), so that a tool whose gateway cannot be found fails rather than runs direct.
    #[must_use]
    pub fn tool_route(&self, id: &ProfileId) -> Option<ToolRoute> {
        let route = self.store.route(Some(id)).ok()?;
        let secrets = route
            .iter()
            .map(|hop| HopSecrets {
                password: self.saved_password_for(&hop.id, &endpoint(hop)),
                passphrase: hop
                    .key_path
                    .as_deref()
                    .and_then(|path| self.saved_passphrase_for(&hop.id, path)),
            })
            .collect();
        let hops = route.iter().map(SshGateway::as_hop).collect();
        Some(ToolRoute::new(hops, secrets, self.ssh_options()))
    }

    /// Applies a message about the Tools area.
    pub(super) fn tools_message(&mut self, message: ToolsMessage) -> Vec<Effect> {
        match message {
            ToolsMessage::Open {
                tool,
                title,
                context,
            } => self.open_tool(tool, title, context),
            ToolsMessage::NewEntry { folder } => {
                self.new_tool_entry(folder);
                Vec::new()
            }
            ToolsMessage::EditEntry(edit) => {
                self.edit_tool_entry(edit);
                Vec::new()
            }
            ToolsMessage::OpenEntry(id) => self.open_tool_entry(&id),
            ToolsMessage::SendCommand {
                target,
                command,
                dangerous,
            } => {
                self.ask_send(target, command, dangerous);
                Vec::new()
            }
            ToolsMessage::InsertCommand { target, command } => {
                self.insert_command(target, &command);
                Vec::new()
            }
            ToolsMessage::BroadcastCommand {
                targets,
                command,
                dangerous,
            } => {
                self.ask_broadcast(&targets, command, dangerous);
                Vec::new()
            }
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
            ToolsMessage::SaveBeforeClose => match self.dialog.take() {
                Some(Dialog::ConfirmCloseTool { tab, .. }) => {
                    self.closing_pane = None;
                    vec![Effect::SaveTool(tab)]
                }
                other => {
                    self.dialog = other;
                    Vec::new()
                }
            },
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

    /// Opens `tool` in a tab named `title`, the last, on `context`, and shows it; a tool
    /// reaching no host already open is shown instead when `context` is empty, as the C#
    /// singleton tools (`MainViewModel.cs:1301-1324`): one given a host or an argument opens
    /// a tab of its own. Either way it becomes the tool used last, as the C# tracks it once
    /// opened.
    pub(super) fn open_tool(
        &mut self,
        tool: ToolId,
        title: String,
        context: ToolContext,
    ) -> Vec<Effect> {
        let open = (!tool.is_network() && context.is_empty())
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
            tab.tool_context = context;
            // Ready at once: nothing connects, as the C# tool tab's "Ready".
            tab.phase = Phase::Connected;
            self.tabs.push(tab);
            self.select_tab(id)
        };
        self.recent_tools.track(tool);
        effects
    }
}
