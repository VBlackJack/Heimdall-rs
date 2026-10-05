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

//! Terminal macros, as the C# ones: what is typed into a session recorded with its pauses,
//! named and kept beside the profiles, then typed again into any session from its tab's
//! menu, as long as wanted, until it ends or is stopped.

use std::time::{Duration, Instant};

use heimdall_core::macros::{MacroEntry, TerminalMacro, macros_path};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{App, Dialog, Effect, Notice, Phase, Tab, TabId};
use crate::macro_player::MacroOutcome;

/// Inputs closer than this after one not ending a line are recorded as one: the letters of
/// a word, not each a step.
const MERGE_WITHIN: Duration = Duration::from_millis(400);

/// A macro being recorded from what is typed into a tab.
#[derive(Debug)]
pub struct MacroRecording {
    entries: Vec<MacroEntry>,
    last: Instant,
}

impl MacroRecording {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            last: Instant::now(),
        }
    }

    /// The inputs recorded so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing was recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `bytes` typed now.
    pub(super) fn note(&mut self, bytes: &[u8]) {
        let now = Instant::now();
        let since = now.duration_since(self.last);
        self.last = now;
        let input = String::from_utf8_lossy(bytes);
        if let Some(previous) = self.entries.last_mut()
            && since < MERGE_WITHIN
            && !previous.input.ends_with('\r')
        {
            previous.input.push_str(&input);
            return;
        }
        let delay_ms = if self.entries.is_empty() {
            0
        } else {
            u32::try_from(since.as_millis()).unwrap_or(u32::MAX)
        };
        self.entries.push(MacroEntry {
            input: input.into_owned(),
            delay_ms,
            expect: None,
        });
    }
}

/// A macro being typed into a tab.
#[derive(Debug)]
pub struct MacroPlaying {
    /// Its name.
    pub name: String,
    stop: CancellationToken,
    output: mpsc::UnboundedSender<Vec<u8>>,
}

impl MacroPlaying {
    /// What the session showed, for what the macro waits for.
    pub(super) fn saw(&self, bytes: &[u8]) {
        let _ = self.output.send(bytes.to_vec());
    }

    /// Stops it where it is.
    pub(super) fn stop(&self) {
        self.stop.cancel();
    }
}

/// A step of the macros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroMessage {
    /// Record what is typed into this tab.
    Record(TabId),
    /// Stop recording it, and ask the macro's name.
    StopRecording(TabId),
    /// The name typed so far.
    NameEdited(String),
    /// Type the macro of this name into this tab.
    Play {
        /// The tab.
        tab: TabId,
        /// The macro.
        name: String,
    },
    /// Stop the macro typed into this tab.
    Stop(TabId),
    /// The macro typed into this tab ended.
    Finished {
        /// The tab.
        tab: TabId,
        /// How.
        outcome: MacroOutcome,
    },
    /// Forget the macro of this name.
    Delete(String),
}

/// What a tab's Macros menu offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroMenu {
    /// Inputs recorded so far, while recording.
    pub recording: Option<usize>,
    /// The macro being typed, while one is.
    pub playing: Option<String>,
    /// The macros kept, by name.
    pub macros: Vec<String>,
}

/// Whether macros can be recorded and typed in `tab`: a terminal connected.
fn takes_macros(tab: &Tab) -> bool {
    tab.phase == Phase::Connected
        && tab.sink.is_some()
        && tab.files.is_none()
        && tab.desktop.is_none()
}

impl App {
    /// The macros kept.
    #[must_use]
    pub fn macros(&self) -> &[TerminalMacro] {
        self.macros.all()
    }

    /// What `tab`'s Macros menu offers; `None` for a tab that takes no macro.
    #[must_use]
    pub fn macro_menu(&self, tab: &Tab) -> Option<MacroMenu> {
        takes_macros(tab).then(|| MacroMenu {
            recording: tab.macro_recording.as_ref().map(MacroRecording::len),
            playing: tab
                .macro_playing
                .as_ref()
                .map(|playing| playing.name.clone()),
            macros: self
                .macros
                .all()
                .iter()
                .map(|kept| kept.name.clone())
                .collect(),
        })
    }

    /// A step of the macros.
    pub(super) fn macro_message(&mut self, message: MacroMessage) -> Vec<Effect> {
        match message {
            MacroMessage::Record(tab) => {
                if let Some(tab) = self.tab_mut(tab).filter(|tab| takes_macros(tab)) {
                    tab.macro_recording = Some(MacroRecording::new());
                }
                Vec::new()
            }
            MacroMessage::StopRecording(tab) => {
                let recorded = self.tab_mut(tab).and_then(|tab| tab.macro_recording.take());
                match recorded {
                    Some(recorded) if !recorded.is_empty() => {
                        self.dialog = Some(Dialog::SaveMacro {
                            name: String::new(),
                            entries: recorded.entries,
                        });
                    }
                    Some(_) => self.tell(Notice::MacroNothingRecorded),
                    None => {}
                }
                Vec::new()
            }
            MacroMessage::NameEdited(typed) => {
                if let Some(Dialog::SaveMacro { name, .. }) = self.dialog.as_mut() {
                    *name = typed;
                }
                Vec::new()
            }
            MacroMessage::Play { tab, name } => self.play_macro(tab, &name),
            MacroMessage::Stop(tab) => {
                if let Some(playing) = self.tab(tab).and_then(|tab| tab.macro_playing.as_ref()) {
                    playing.stop();
                }
                Vec::new()
            }
            MacroMessage::Finished { tab, outcome } => {
                let ended = self.tab_mut(tab).and_then(|tab| tab.macro_playing.take());
                if let Some(ended) = ended {
                    self.tell(Notice::MacroEnded {
                        name: ended.name,
                        outcome,
                    });
                }
                Vec::new()
            }
            MacroMessage::Delete(name) => {
                let mut kept = self.macros.clone();
                if kept.remove(&name) {
                    self.keep_macros(kept, Notice::MacroDeleted(name));
                }
                Vec::new()
            }
        }
    }

    /// The macro recorded, named as the user typed: kept, in place of one of that name.
    pub(super) fn save_macro(&mut self, name: &str, entries: Vec<MacroEntry>) {
        let name = name.trim();
        if name.is_empty() {
            // Asked again, nothing lost.
            self.dialog = Some(Dialog::SaveMacro {
                name: String::new(),
                entries,
            });
            return;
        }
        let mut kept = self.macros.clone();
        kept.put(TerminalMacro {
            name: name.to_owned(),
            entries,
        });
        self.keep_macros(kept, Notice::MacroSaved(name.to_owned()));
    }

    /// `kept` saved beside the profiles, then in use, `done` said; a failure said instead.
    fn keep_macros(&mut self, kept: heimdall_core::macros::Macros, done: Notice) {
        match kept.save(&macros_path(&self.config.profiles_file)) {
            Ok(()) => {
                self.macros = kept;
                self.tell(done);
            }
            Err(error) => {
                self.dialog = Some(Dialog::StoreError {
                    detail: error.to_string(),
                });
            }
        }
    }

    /// The macro `name` typed into `tab`, one at a time.
    fn play_macro(&mut self, tab_id: TabId, name: &str) -> Vec<Effect> {
        let Some(found) = self.macros.get(name).cloned() else {
            return Vec::new();
        };
        let Some(tab) = self
            .tab_mut(tab_id)
            .filter(|tab| takes_macros(tab) && tab.macro_playing.is_none())
        else {
            return Vec::new();
        };
        let Some(sink) = tab.sink.clone() else {
            return Vec::new();
        };
        let (output, shown) = mpsc::unbounded_channel();
        let stop = CancellationToken::new();
        tab.macro_playing = Some(MacroPlaying {
            name: found.name.clone(),
            stop: stop.clone(),
            output,
        });
        vec![Effect::PlayMacro {
            tab: tab_id,
            run: Box::pin(crate::macro_player::play(found.entries, sink, shown, stop)),
        }]
    }
}
