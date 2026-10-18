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

//! A tool saved as a tree entry, as the C# "Add > Tool" (`MainWindow.xaml.cs:921-961`,
//! `ShowAddToolPickerAsync`): a picker over the tools, a host for a tool that reaches one,
//! and the entry written to the profiles. Opening the entry opens the tool's tab on the
//! host and port it holds, as the C# `OpenSessionTreeToolTab`
//! (`MainWindow.TreeInteractions.cs:396-405`).

use heimdall_core::profile::{ProfileId, ToolProfile};
use heimdall_core::tools::net_host::checked_host;

use super::{App, Dialog, Effect, Notice};
use crate::text::server_text;
use crate::tools::{ToolContext, ToolId};

/// The picker's state: what is searched, the tool chosen and the host typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolEntryForm {
    /// What the list of tools is narrowed by.
    pub query: String,
    /// The tool chosen, with the name its entry will bear (its name in the language shown, as
    /// the C# `SelectedDisplayName`).
    pub selected: Option<(ToolId, String)>,
    /// The host typed, for a tool that reaches one.
    pub host: String,
    /// The folder the entry is filed in.
    pub folder: Option<String>,
}

/// What the picker is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolEntryEdit {
    /// The search box changed.
    Query(String),
    /// A tool was chosen; carries the name its entry will bear.
    Pick(ToolId, String),
    /// The host box changed.
    Host(String),
}

impl App {
    /// Opens the picker, the host prefilled with the selected session's as the C# does.
    pub(super) fn new_tool_entry(&mut self, folder: Option<String>) {
        let host = self.tool_target_host().unwrap_or_default();
        self.dialog = Some(Dialog::AddTool(Box::new(ToolEntryForm {
            host,
            folder,
            ..ToolEntryForm::default()
        })));
    }

    /// Applies an edit of the picker.
    pub(super) fn edit_tool_entry(&mut self, edit: ToolEntryEdit) {
        let Some(Dialog::AddTool(form)) = self.dialog.as_mut() else {
            return;
        };
        match edit {
            ToolEntryEdit::Query(query) => form.query = query,
            ToolEntryEdit::Pick(tool, name) => form.selected = Some((tool, name)),
            ToolEntryEdit::Host(host) => form.host = host,
        }
    }

    /// Writes the entry the picker describes. Nothing chosen leaves the picker open, as the
    /// C# dialog's disabled "Add"; a host a network tool cannot reach is left out, as the C#
    /// leaves out the host of a tool that is not a network one.
    pub(super) fn confirm_tool_entry(&mut self, form: ToolEntryForm) {
        let Some((tool, name)) = form.selected.clone() else {
            self.dialog = Some(Dialog::AddTool(Box::new(form)));
            return;
        };
        let host = tool
            .is_network()
            .then(|| form.host.trim().to_owned())
            .filter(|host| !host.is_empty());
        let id = self.fresh_id();
        let entry = ToolProfile {
            id: id.clone(),
            name: server_text(&name),
            group: form.folder.clone(),
            tool: tool.code().to_owned(),
            host,
            port: None,
        };
        match self.store.apply(|store| store.merge_tools([entry])) {
            Ok(_) => self.selected_profile = Some(id),
            Err(error) => self.dialog = Some(Dialog::save_failed(&error)),
        }
    }

    /// Opens the tool of entry `id` in a tab named as the entry, on the host and port it
    /// holds: the host given to a tool that reaches it goes through the checks a typed one
    /// goes through. An entry naming a tool this build does not have says so.
    pub(super) fn open_tool_entry(&mut self, id: &ProfileId) -> Vec<Effect> {
        let Some(entry) = self
            .store
            .tool_profiles()
            .iter()
            .find(|entry| &entry.id == id)
            .cloned()
        else {
            return Vec::new();
        };
        let Some(tool) = ToolId::from_code(&entry.tool) else {
            self.tell(Notice::UnknownTool(entry.tool));
            return Vec::new();
        };
        let mut context = entry
            .host
            .as_deref()
            .filter(|_| tool.is_network())
            .and_then(checked_host)
            .map_or_else(ToolContext::default, |host| ToolContext::on_host(&host));
        context.port = entry.port.filter(|_| context.host.is_some());
        self.open_tool(tool, entry.name, context)
    }
}
