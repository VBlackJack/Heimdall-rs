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

/// The most receipts kept until the window takes them: it takes them after each message.
const MAX_RECEIPTS: usize = 256;

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

/// What is asked before one command is written into several terminals at once, as the C#
/// `CommandLibraryViewModel.BroadcastAsync` asks (`...Broadcast.cs:122-162`): one question
/// with the command and every terminal that receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcastConfirmation {
    /// The terminals, in the order of the strip.
    pub targets: Vec<SendTarget>,
    /// The command as it is typed.
    pub command: String,
    /// The command as it is shown, every invisible character written out.
    pub shown: String,
    /// The command is marked dangerous.
    pub dangerous: bool,
}

/// A command written into a terminal once the user agreed, which the window that asked
/// records in its history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentCommand {
    /// The terminal tab.
    pub tab: TabId,
    /// Its name, made safe.
    pub target: String,
    /// The command as it was typed.
    pub command: String,
}

impl App {
    /// The receipts of the commands written since the last call, taken.
    pub fn take_sent_commands(&mut self) -> Vec<SentCommand> {
        std::mem::take(&mut self.sent_commands)
    }

    fn keep_receipt(&mut self, receipt: SentCommand) {
        if self.sent_commands.len() >= MAX_RECEIPTS {
            self.sent_commands.remove(0);
        }
        self.sent_commands.push(receipt);
    }

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

    /// Asks whether to write `command` into every terminal of `targets` still offered. A
    /// command a single send would refuse, or no terminal left, is refused without a
    /// question; nothing is written before it is agreed to.
    pub(super) fn ask_broadcast(&mut self, targets: &[TabId], command: String, dangerous: bool) {
        if command.trim().is_empty() || command.chars().any(char::is_control) {
            return;
        }
        let chosen: Vec<SendTarget> = self
            .send_targets()
            .into_iter()
            .filter(|each| targets.contains(&each.tab))
            .collect();
        if chosen.is_empty() {
            return;
        }
        self.dialog = Some(Dialog::ConfirmBroadcastCommand(Box::new(
            BroadcastConfirmation {
                targets: chosen,
                shown: visible_text(&command),
                command,
                dangerous,
            },
        )));
    }

    /// Writes the command agreed to into its terminal, then Enter, while that terminal still
    /// takes input.
    pub(super) fn confirm_send(&mut self, confirmation: &SendConfirmation) -> Vec<Effect> {
        self.write_command(
            confirmation.tab,
            &confirmation.target,
            &confirmation.command,
        );
        Vec::new()
    }

    /// Writes the command agreed to into each terminal of the question that still takes
    /// input: a terminal closed meanwhile is left out, never replaced by the one that took
    /// its place.
    pub(super) fn confirm_command_broadcast(
        &mut self,
        confirmation: &BroadcastConfirmation,
    ) -> Vec<Effect> {
        for target in &confirmation.targets {
            self.write_command(target.tab, &target.name, &confirmation.command);
        }
        Vec::new()
    }

    /// Types `command` and Enter into `tab` if it still takes input, and leaves a receipt.
    fn write_command(&mut self, tab: TabId, name: &str, command: &str) {
        let Some(found) = self
            .tab_mut(tab)
            .filter(|tab| super::broadcast::takes_input(tab))
        else {
            return;
        };
        let mut bytes = encode_paste(command, &found.terminal.input_mode());
        bytes.push(SUBMIT);
        found.write(bytes);
        self.keep_receipt(SentCommand {
            tab,
            target: name.to_owned(),
            command: command.to_owned(),
        });
    }
}
