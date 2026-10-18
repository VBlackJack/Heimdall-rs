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

//! "Send" of the command library: a command written into a terminal tab, as the C#
//! `CommandLibraryViewModel.SendAsync` (`CommandLibraryViewModel.Commands.cs:62-80`) writes
//! it into the terminal beside the tool.
//!
//! The C# asks only for a command marked dangerous, and finds its terminal in the split of
//! the tool's own tab (`EmbeddedSessionManager.cs:1623-1630`, the first terminal of the
//! tree). A tool is a tab of its own here, never split with a session, so there is no such
//! tree: the terminals are offered and the user names one. Every command is asked about,
//! shown whole with the terminal that receives it, and nothing is written before the
//! question is answered. The command is typed as one line and submitted with Enter, as the
//! C# `TerminalCommandFormatter.Submit` does, once agreed to.

use heimdall_term::encode_paste;

use super::{App, Dialog, Effect};
use crate::ids::TabId;
use crate::text::{server_text, visible_text};

/// What a terminal receives after the command, to run it: Enter, as a typed key.
const SUBMIT: u8 = b'\r';

/// A terminal tab that can receive a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendTarget {
    /// The tab.
    pub tab: TabId,
    /// Its name, made safe.
    pub name: String,
}

/// What the user is asked before a command is written into a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendConfirmation {
    /// The terminal tab.
    pub tab: TabId,
    /// Its name, made safe.
    pub target: String,
    /// The command as it is typed.
    pub command: String,
    /// The command as it is shown, every invisible character written out.
    pub shown: String,
    /// The command is marked dangerous.
    pub dangerous: bool,
}

impl App {
    /// The terminal tabs a command can be written into: a session of text, connected,
    /// asking nothing, as broadcast input reaches them, in the order of the strip.
    #[must_use]
    pub fn send_targets(&self) -> Vec<SendTarget> {
        self.tabs
            .iter()
            .filter(|tab| super::broadcast::takes_input(tab))
            .map(|tab| SendTarget {
                tab: tab.id,
                name: server_text(&tab.title),
            })
            .collect()
    }

    /// Asks whether to write `command` into terminal `target`. A command with a line break
    /// or another control character, an empty one, or a terminal that no longer takes input
    /// is refused without a question: nothing is ever written before the question is agreed
    /// to.
    pub(super) fn ask_send(&mut self, target: TabId, command: String, dangerous: bool) {
        if command.trim().is_empty() || command.chars().any(char::is_control) {
            return;
        }
        let Some(found) = self
            .send_targets()
            .into_iter()
            .find(|each| each.tab == target)
        else {
            return;
        };
        self.dialog = Some(Dialog::ConfirmSendCommand(Box::new(SendConfirmation {
            tab: target,
            target: found.name,
            shown: visible_text(&command),
            command,
            dangerous,
        })));
    }

    /// Writes the command agreed to into its terminal, then Enter, while that terminal still
    /// takes input.
    pub(super) fn confirm_send(&mut self, confirmation: &SendConfirmation) -> Vec<Effect> {
        let Some(tab) = self
            .tab_mut(confirmation.tab)
            .filter(|tab| super::broadcast::takes_input(tab))
        else {
            return Vec::new();
        };
        let mut bytes = encode_paste(&confirmation.command, &tab.terminal.input_mode());
        bytes.push(SUBMIT);
        tab.write(bytes);
        Vec::new()
    }
}
