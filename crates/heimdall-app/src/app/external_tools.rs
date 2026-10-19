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

//! The user's external tools started from a session's menu, its tab's menu and Quick
//! Connect, as the C# "External Tools" submenu (`ContextMenuFactory.cs:496-526`) and palette
//! (`CommandPaletteViewModel.cs:593-602`): the placeholders filled from the session, or,
//! from the palette with no session selected and from the Settings "Test" button, the
//! template as written. How the start went is said on the status bar, as the C# says a
//! failure in a warning.

use heimdall_core::external_tools::{ExternalTool, ToolTarget};
use heimdall_core::profile::ProfileId;

use super::{App, Effect, GatewayBadge, Notice, ProfileKind};
use crate::external_tool::{self, Launched, ToolFailure};

/// Something about an external tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalToolMessage {
    /// Start the tool of the list named `name`, for the profile `profile`; with none, its
    /// template as written, as the C# palette with no session selected.
    Launch {
        /// The tool, by its name, unique in the list.
        name: String,
        /// The session it is started for.
        profile: Option<ProfileId>,
    },
    /// Start `tool` as the Settings editor holds it, for no session, as the C# "Test".
    Test(ExternalTool),
    /// How the start of the tool named `name` went.
    Launched {
        /// The tool.
        name: String,
        /// Started, declined at the elevation prompt, or why not.
        result: Result<Launched, ToolFailure>,
    },
}

impl App {
    /// Applies a message about an external tool.
    pub(super) fn external_tool_message(&mut self, message: ExternalToolMessage) -> Vec<Effect> {
        match message {
            ExternalToolMessage::Launch { name, profile } => {
                let Some(tool) = self.external_tool(&name).cloned() else {
                    return Vec::new();
                };
                let target = match profile {
                    Some(id) => match self.tool_target(&id) {
                        Some(target) => Some(target),
                        // Deleted since its menu was drawn: nothing to start it for.
                        None => return Vec::new(),
                    },
                    None => None,
                };
                self.launch_tool(&tool, target.as_ref())
            }
            ExternalToolMessage::Test(tool) => self.launch_tool(&tool, None),
            ExternalToolMessage::Launched { name, result } => {
                match result {
                    Ok(Launched::Started) => self.tell(Notice::ExternalToolStarted(name)),
                    // As the C#, which only logs a prompt declined.
                    Ok(Launched::Declined) => {
                        log::info!("external tool {name:?}: the elevation prompt was declined");
                    }
                    Err(failure) => self.tell(Notice::ExternalToolFailed { name, failure }),
                }
                Vec::new()
            }
        }
    }

    /// The effect starting `tool` for `target`, or the failure said at once when it cannot
    /// start.
    fn launch_tool(&mut self, tool: &ExternalTool, target: Option<&ToolTarget>) -> Vec<Effect> {
        let name = tool.name.trim().to_owned();
        match external_tool::prepare(tool, target, &self.config.files_start) {
            Ok(launch) => vec![Effect::LaunchExternalTool {
                name,
                launch: Box::new(launch),
            }],
            Err(failure) => {
                log::warn!("external tool {name:?} not started: {failure:?}");
                self.tell(Notice::ExternalToolFailed { name, failure });
                Vec::new()
            }
        }
    }

    /// The user's external tools, in the order the menus list them.
    #[must_use]
    pub fn external_tools(&self) -> &[ExternalTool] {
        &self.settings.external_tools
    }

    /// The tool of the list named `name`.
    fn external_tool(&self, name: &str) -> Option<&ExternalTool> {
        self.settings
            .external_tools
            .iter()
            .find(|tool| tool.name.trim() == name.trim())
    }

    /// What the placeholders stand for when a tool is started for profile `id`, as the C#
    /// reads them from the server's row: its address and port, account, name, protocol,
    /// the key file of an SSH or SFTP session, and the gateway it goes through. The C#
    /// project, which this version does not have, is empty.
    #[must_use]
    pub fn tool_target(&self, id: &ProfileId) -> Option<ToolTarget> {
        let profile = self.profile_summary(id)?;
        let (host, port) = profile
            .endpoint
            .map_or((String::new(), None), |(host, port)| (host, Some(port)));
        let key_file = if matches!(profile.kind, ProfileKind::Ssh | ProfileKind::Sftp) {
            self.store
                .ssh_profiles()
                .iter()
                .find(|ssh| ssh.id == *id)
                .and_then(|ssh| ssh.key_path.as_ref())
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        Some(ToolTarget {
            host,
            port,
            user: profile.username.unwrap_or_default(),
            server_name: profile.name,
            protocol: external_tool::protocol_name(profile.kind).to_owned(),
            key_file,
            project: String::new(),
            gateway: match profile.gateway {
                Some(GatewayBadge::Via(name)) => name,
                Some(GatewayBadge::Missing) | None => String::new(),
            },
        })
    }

    /// The command `tool` would run, as the C# Settings preview shows it
    /// (`ExternalToolSettingsService.BuildPreview`): for the session selected in the tree,
    /// else its template as written. Drawn with the page, it reads nothing on disk: whether
    /// the program is there is said when it starts.
    ///
    /// # Errors
    ///
    /// [`ToolFailure::OptionLike`] when a value of that session would read as an option.
    pub fn external_tool_preview(&self, tool: &ExternalTool) -> Result<String, ToolFailure> {
        let target = self
            .selected_profile
            .as_ref()
            .and_then(|id| self.tool_target(id));
        external_tool::preview(tool, target.as_ref())
    }
}
