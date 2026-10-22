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

//! "Copy output, secrets masked", as the C# `MaskedTerminalCopyService` and
//! `MaskedCopyDialogViewModel` (PR #647): the text of a terminal session is read, masked by
//! [`heimdall_core::redaction`], and shown for review; only the Copy button puts it on the
//! clipboard. When the engine gives up (a rule ran out of time) nothing can be copied, and
//! the unmasked text is never offered instead.
//!
//! Offline by construction: no network and nothing written to disk. The unmasked text lives
//! in memory for as long as the dialog is open, to mask it again when the option to
//! pseudonymize hosts changes, and neither it nor the masked text is ever put in a log, in a
//! notice or in a `Debug` output.

use std::sync::Arc;

use heimdall_core::redaction::{Redacted, RedactionError, RedactionOptions};

use super::tree::ProfileKind;
use super::{App, Dialog, Effect, Notice, Phase, Tab};
use crate::driver::Purpose;
use crate::ids::TabId;

/// What the user does in the dialog, and what the masking reports.
#[derive(Debug, Clone)]
pub enum MaskedCopyMessage {
    /// Read the text of a tab's terminal and open the dialog on it.
    Open(TabId),
    /// The option that also masks host names and addresses was switched.
    HostsChanged(bool),
    /// A masking finished.
    Done {
        /// The masking, as [`Effect::Redact`] numbered it.
        run: u64,
        /// What the engine returned.
        outcome: Result<Redacted, RedactionError>,
    },
    /// A masking could not finish at all (its thread failed): nothing can be copied.
    Failed {
        /// The masking, as [`Effect::Redact`] numbered it.
        run: u64,
    },
}

/// What the dialog shows of the masking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskedView {
    /// The text is being masked: nothing can be copied yet.
    Working,
    /// The text is masked.
    Ready(Redacted),
    /// A rule ran out of time: nothing can be copied.
    Refused {
        /// The id of the rule that gave up.
        rule: &'static str,
    },
    /// The masking could not run: nothing can be copied.
    Failed,
}

/// The dialog "Copy output, secrets masked".
#[derive(Clone, PartialEq, Eq)]
pub struct MaskedCopy {
    /// The tab whose text it is.
    pub tab: TabId,
    /// Host names and IPv4 addresses are replaced by pseudonyms too.
    pub pseudonymize_hosts: bool,
    /// What the masking gave.
    pub view: MaskedView,
    /// The masking whose answer is waited for; an older answer is dropped.
    run: u64,
    /// The unmasked text, kept to mask it again.
    source: Arc<str>,
}

/// Debug never prints the text: it is the unmasked terminal output.
impl std::fmt::Debug for MaskedCopy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskedCopy")
            .field("tab", &self.tab)
            .field("pseudonymize_hosts", &self.pseudonymize_hosts)
            .field("view", &self.view)
            .field("source_bytes", &self.source.len())
            .finish_non_exhaustive()
    }
}

impl MaskedCopy {
    /// The text to copy, once it is masked.
    #[must_use]
    pub fn masked_text(&self) -> Option<&str> {
        match &self.view {
            MaskedView::Ready(redacted) => Some(&redacted.text),
            MaskedView::Working | MaskedView::Refused { .. } | MaskedView::Failed => None,
        }
    }
}

impl App {
    /// Whether `tab` shows a session of text whose output "Copy output, secrets masked" can
    /// read: an SSH, Telnet, `WinRM` or local shell, connected or ended. Never a remote
    /// desktop, a Files tab, a web console or a tool.
    #[must_use]
    pub fn can_copy_masked(&self, tab: &Tab) -> bool {
        matches!(
            self.tab_kind(tab),
            ProfileKind::Ssh | ProfileKind::Telnet | ProfileKind::WinRm | ProfileKind::Local
        ) && tab.purpose == Purpose::Shell
            && tab.files.is_none()
            && tab.desktop.is_none()
            && tab.elevated.is_none()
            && matches!(tab.phase, Phase::Connected | Phase::Closed { .. })
    }

    /// Applies a message about the masked copy.
    pub(super) fn masked_copy_message(&mut self, message: MaskedCopyMessage) -> Vec<Effect> {
        match message {
            MaskedCopyMessage::Open(tab) => self.open_masked_copy(tab),
            MaskedCopyMessage::HostsChanged(on) => self.set_masked_hosts(on),
            MaskedCopyMessage::Done { run, outcome } => {
                self.masked_copy_done(run, Some(outcome));
                Vec::new()
            }
            MaskedCopyMessage::Failed { run } => {
                self.masked_copy_done(run, None);
                Vec::new()
            }
        }
    }

    /// Reads the text of `tab`'s terminal and starts masking it; another dialog open keeps
    /// the keyboard, and the key does nothing.
    fn open_masked_copy(&mut self, tab_id: TabId) -> Vec<Effect> {
        if self.dialog.is_some() {
            return Vec::new();
        }
        let Some(tab) = self.tab(tab_id).filter(|tab| self.can_copy_masked(tab)) else {
            self.tell(Notice::MaskedCopyNoTerminal);
            return Vec::new();
        };
        let source: Arc<str> = tab.terminal.session_text().into();
        let pseudonymize_hosts = false;
        let run = self.start_masked_run();
        let effect = Effect::Redact {
            run,
            text: Arc::clone(&source),
            options: RedactionOptions { pseudonymize_hosts },
        };
        self.dialog = Some(Dialog::MaskedCopy(Box::new(MaskedCopy {
            tab: tab_id,
            pseudonymize_hosts,
            view: MaskedView::Working,
            run,
            source,
        })));
        vec![effect]
    }

    /// Masks the text again with the host option as asked; nothing can be copied until the
    /// new masking is shown, so the clipboard never gets a text the screen does not show.
    fn set_masked_hosts(&mut self, on: bool) -> Vec<Effect> {
        if !matches!(&self.dialog, Some(Dialog::MaskedCopy(state)) if state.pseudonymize_hosts != on)
        {
            return Vec::new();
        }
        let run = self.start_masked_run();
        let Some(Dialog::MaskedCopy(state)) = &mut self.dialog else {
            return Vec::new();
        };
        state.pseudonymize_hosts = on;
        state.view = MaskedView::Working;
        state.run = run;
        vec![Effect::Redact {
            run,
            text: Arc::clone(&state.source),
            options: RedactionOptions {
                pseudonymize_hosts: on,
            },
        }]
    }

    /// The number of a new masking.
    fn start_masked_run(&mut self) -> u64 {
        self.next_masked_copy += 1;
        self.next_masked_copy
    }

    /// Shows what masking `run` gave, unless a newer one was asked for or the dialog is gone.
    fn masked_copy_done(&mut self, run: u64, outcome: Option<Result<Redacted, RedactionError>>) {
        let Some(Dialog::MaskedCopy(state)) = &mut self.dialog else {
            return;
        };
        if state.run != run {
            return;
        }
        state.view = match outcome {
            Some(Ok(redacted)) => MaskedView::Ready(redacted),
            Some(Err(RedactionError::Timeout { rule })) => MaskedView::Refused { rule },
            None => MaskedView::Failed,
        };
    }

    /// Copy: the masked text, exactly as shown, goes to the clipboard and the dialog closes.
    /// Nothing is copied while the text is being masked again or when the engine refused.
    pub(super) fn confirm_masked_copy(&mut self) -> Vec<Effect> {
        let Some(text) = (match &self.dialog {
            Some(Dialog::MaskedCopy(state)) => state.masked_text().map(str::to_owned),
            _ => None,
        }) else {
            return Vec::new();
        };
        self.dialog = None;
        self.tell(Notice::MaskedCopyCopied);
        vec![Effect::WriteClipboard(text)]
    }
}
