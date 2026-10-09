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

//! The Password Generator's tab, as the C# `PasswordGeneratorView`
//! (`PasswordGeneratorView.xaml`, `PasswordGeneratorView.xaml.cs`) over the engine of
//! [`heimdall_core::tools::password_generator`]: the mode, the presets built in and saved,
//! the minimum strength, the advanced options; the password with Generate and Copy, its
//! strength, crack time, issues and notices; each mode's controls; the case blocks; the
//! placement bar, its cursors dragged, wheeled or moved with the arrows; the batch; the
//! phonetic reading; the history; the clipboard cleared after its delay.
//!
//! Passwords are secrets: held in memory wiped when replaced or when the tab closes, never
//! logged nor kept on disk; a batch is written to a file only where the user exports it.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use heimdall_app::TabId;
use heimdall_core::tools::password_generator::{
    CLIPBOARD_CLEAR_CHOICES, ENTROPY_FLOOR_CHOICES, FloorSearchNotice, GeneratorMode, Issue,
    MAXIMUM_BATCH_COUNT, MAXIMUM_LEET_EXTRAS, MAXIMUM_LENGTH, MAXIMUM_PASSPHRASE_WORDS,
    MAXIMUM_SYLLABLE_EXTRAS, MAXIMUM_SYLLABLE_LENGTH, MINIMUM_BATCH_COUNT, MINIMUM_LENGTH,
    MINIMUM_PASSPHRASE_WORDS, MINIMUM_SYLLABLE_LENGTH, Notice, PasswordGenerator, QUICK_LENGTHS,
    SYLLABLE_LENGTH_STEP, Setting, SpecialsNotice, StrengthLevel,
};
use heimdall_core::tools::password_presets;
use heimdall_core::tools::password_rules::{
    self as rules, CASE_BLOCK_TOKENS, CaseMode, CrackTime, Placement,
};
use heimdall_core::tools::password_wordlists::PASSPHRASE_LANGUAGES;
use heimdall_core::tools::private_file;
use iced::widget::text_editor::{Action, Content};
use iced::widget::{
    Column, button, canvas, checkbox, column, container, pick_list, progress_bar, row, slider,
    text, text_input, tooltip,
};
use iced::{Alignment, Color, Element, Length, Task, Theme, window};
use zeroize::Zeroizing;

use super::crypto_parts::{self, Tone};
use super::key_parts;
use super::placement_bar::{PlacementTrack, TRACK_HEIGHT};
use super::{CopySlot, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// What separates the issues, as the C#'s `"  •  "`.
const ISSUE_SEPARATOR: &str = "  \u{2022}  ";

/// What separates the notices, as the C#'s `" "`.
const NOTICE_SEPARATOR: &str = " ";

/// How long "Clipboard cleared" stays before the keyboard hint is back, as the C#'s 3 s.
const CLEARED_SHOWN: Duration = Duration::from_secs(3);

/// A second of the clipboard's countdown, as the C# hint timer's.
const TICK: Duration = Duration::from_secs(1);

/// Height of the password's box, as the C# `FontSizeLarge` line and its padding.
const OUTPUT_HEIGHT: f32 = 44.0;

/// Height of the strength bar, as the C# `Height="10"`.
const STRENGTH_HEIGHT: f32 = 10.0;

/// Width of a slider's value, as the C# `Width="40"`.
const VALUE_WIDTH: f32 = 40.0;

/// Width of the short text boxes, as the C# `Width="60"`.
const SHORT_FIELD_WIDTH: f32 = 60.0;

/// Width of a block's button, as the C# `MinWidth="34"`.
const BLOCK_WIDTH: f32 = 34.0;

/// The longest separator typed, as the C# `MaxLength="4"`.
const SEPARATOR_MAX: usize = 4;

/// The longest leet base word typed, as the C# `MaxLength="32"`.
const LEET_WORD_MAX: usize = 32;

/// The longest custom specials typed, as the C# `MaxLength="64"`.
const SPECIALS_MAX: usize = 64;

/// The file a batch is offered under, as the C#'s `passwords_{now:yyyyMMdd_HHmmss}.txt`.
const EXPORT_PREFIX: &str = "passwords_";
const EXPORT_EXTENSION: &str = "txt";
const EXPORT_EXTENSIONS: &[&str] = &[EXPORT_EXTENSION];

/// A line break of an export, as .NET's `Environment.NewLine`.
#[cfg(windows)]
const NEW_LINE: &str = "\r\n";
#[cfg(not(windows))]
const NEW_LINE: &str = "\n";

/// A built-in preset, as the C# `BuiltInPresets` table (`PasswordGeneratorView.xaml.cs:631-655`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltIn {
    Pin4,
    Pin6,
    Wifi,
    ApiKey,
    Mysql,
    Ssh,
    SylEasy,
    SylBalanced,
    SylStrong,
    Passphrase4,
    Passphrase6,
}

impl BuiltIn {
    /// The built-in presets of `mode`, as the C# table.
    fn of(mode: GeneratorMode) -> &'static [Self] {
        match mode {
            GeneratorMode::Random => &[
                Self::Pin4,
                Self::Pin6,
                Self::Wifi,
                Self::ApiKey,
                Self::Mysql,
                Self::Ssh,
            ],
            GeneratorMode::Syllable => &[Self::SylEasy, Self::SylBalanced, Self::SylStrong],
            GeneratorMode::Passphrase => &[Self::Passphrase4, Self::Passphrase6],
            GeneratorMode::Leet => &[],
        }
    }

    /// It applied, as the C# table's actions.
    fn apply(self, engine: &mut PasswordGenerator) {
        match self {
            Self::Pin4 => engine.apply_random_preset(4, false, false, true, false),
            Self::Pin6 => engine.apply_random_preset(6, false, false, true, false),
            Self::Wifi => engine.apply_random_preset(63, true, true, true, true),
            Self::ApiKey => engine.apply_random_preset(32, true, false, true, false),
            Self::Mysql => engine.apply_random_preset(16, true, true, true, false),
            Self::Ssh => engine.apply_random_preset(20, true, true, true, true),
            Self::SylEasy => engine.apply_syllable_preset(18, CaseMode::Title, 1, 0, "-", false),
            Self::SylBalanced => {
                engine.apply_syllable_preset(24, CaseMode::Mixed, 2, 1, "-", true);
            }
            Self::SylStrong => engine.apply_syllable_preset(30, CaseMode::Mixed, 3, 2, "", true),
            Self::Passphrase4 => engine.apply_passphrase_preset(4, "-"),
            Self::Passphrase6 => engine.apply_passphrase_preset(6, "-"),
        }
    }

    fn label(self) -> String {
        match self {
            Self::Pin4 => fl!("ui-tool-password-preset-pin4"),
            Self::Pin6 => fl!("ui-tool-password-preset-pin6"),
            Self::Wifi => fl!("ui-tool-password-preset-wifi"),
            Self::ApiKey => fl!("ui-tool-password-preset-api-key"),
            Self::Mysql => fl!("ui-tool-password-preset-mysql"),
            Self::Ssh => fl!("ui-tool-password-preset-ssh"),
            Self::SylEasy => fl!("ui-tool-password-preset-syl-easy"),
            Self::SylBalanced => fl!("ui-tool-password-preset-syl-balanced"),
            Self::SylStrong => fl!("ui-tool-password-preset-syl-strong"),
            Self::Passphrase4 => fl!("ui-tool-password-preset-passphrase4"),
            Self::Passphrase6 => fl!("ui-tool-password-preset-passphrase6"),
        }
    }
}

/// An entry of the preset list, as the C# `PresetEntry`: the first stands for settings that
/// are nobody's preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetEntry {
    /// The settings as they are.
    Custom,
    /// A preset the tool ships.
    BuiltIn(BuiltIn),
    /// A preset the user saved, by its name.
    Saved(String),
}

impl fmt::Display for PresetEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Custom => f.write_str(&fl!("ui-tool-password-preset-custom")),
            Self::BuiltIn(preset) => f.write_str(&preset.label()),
            Self::Saved(name) => {
                write!(
                    f,
                    "{}: {name}",
                    fl!("ui-tool-password-presets-saved").trim_end_matches(':')
                )
            }
        }
    }
}

/// A mode in its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeChoice(pub GeneratorMode);

impl fmt::Display for ModeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            GeneratorMode::Random => fl!("ui-tool-password-mode-random"),
            GeneratorMode::Syllable => fl!("ui-tool-password-mode-syllable"),
            GeneratorMode::Passphrase => fl!("ui-tool-password-mode-passphrase"),
            GeneratorMode::Leet => fl!("ui-tool-password-mode-leet"),
        })
    }
}

/// A case mode in its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaseChoice(pub CaseMode);

impl fmt::Display for CaseChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            CaseMode::Mixed => fl!("ui-tool-password-case-mixed"),
            CaseMode::Lower => fl!("ui-tool-password-case-lower"),
            CaseMode::Upper => fl!("ui-tool-password-case-upper"),
            CaseMode::Title => fl!("ui-tool-password-case-title"),
            CaseMode::Alternating => fl!("ui-tool-password-case-alternating"),
            CaseMode::WordCase => fl!("ui-tool-password-case-word"),
            CaseMode::Inverse => fl!("ui-tool-password-case-inverse"),
            CaseMode::Blocks => fl!("ui-tool-password-case-blocks"),
        })
    }
}

/// A placement in its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementChoice(pub Placement);

impl fmt::Display for PlacementChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            Placement::Random => fl!("ui-tool-password-placement-random"),
            Placement::Start => fl!("ui-tool-password-placement-start"),
            Placement::End => fl!("ui-tool-password-placement-end"),
            Placement::Middle => fl!("ui-tool-password-placement-middle"),
            Placement::Positions => fl!("ui-tool-password-placement-positions"),
        })
    }
}

/// A passphrase language in its box, by its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageChoice(pub usize);

impl fmt::Display for LanguageChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&match self.0 {
            1 => fl!("ui-tool-password-lang-french"),
            2 => fl!("ui-tool-password-lang-spanish"),
            3 => fl!("ui-tool-password-lang-latin"),
            _ => fl!("ui-tool-password-lang-english"),
        })
    }
}

/// A minimum strength in its box, by its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloorChoice(pub usize);

impl fmt::Display for FloorChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match ENTROPY_FLOOR_CHOICES.get(self.0).copied().unwrap_or(0) {
            0 => f.write_str(&fl!("ui-tool-password-entropy-floor-off")),
            bits => write!(f, "{bits} {}", fl!("ui-tool-password-bits")),
        }
    }
}

/// A clipboard delay in its box, by its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelayChoice(pub usize);

impl fmt::Display for DelayChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let seconds = CLIPBOARD_CLEAR_CHOICES.get(self.0).copied().unwrap_or(0);
        write!(f, "{seconds} {}", fl!("ui-tool-password-seconds"))
    }
}

/// What a copy button copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordCopy {
    /// The password shown.
    Main,
    /// Its phonetic reading.
    Phonetic,
    /// The whole batch.
    BatchAll,
    /// A row of the batch.
    BatchRow(usize),
    /// A password of the history.
    History(usize),
}

/// What the Password Generator is asked.
#[derive(Clone)]
pub enum PasswordMessage {
    /// A setting changed.
    Set(Setting),
    /// Another password, as the C# Generate button.
    Generate,
    /// The output cleared, as the C# Escape.
    ClearOutput,
    /// A copy button pressed.
    Copy(PasswordCopy),
    /// The advanced options shown or hidden.
    ToggleAdvanced,
    /// A preset picked.
    Preset(PresetEntry),
    /// Save the settings as a preset: its name asked first.
    SavePreset,
    /// The name typed.
    PresetName(String),
    /// The name given.
    PresetNameOk,
    /// The name not given.
    PresetNameCancel,
    /// Delete the saved preset picked: confirmed first.
    DeletePreset,
    /// The deletion confirmed.
    DeleteConfirm,
    /// The deletion not confirmed.
    DeleteCancel,
    /// A quick length pressed.
    QuickLength(usize),
    /// One block more.
    CaseBlockAdd,
    /// One block fewer.
    CaseBlockRemove,
    /// The blocks drawn.
    CaseBlocksRandom,
    /// Every block set to this token.
    CaseBlocksAll(char),
    /// This block moved on to its next token.
    CaseBlockCycle(usize),
    /// The cursors spread evenly.
    Distribute,
    /// A cursor moved: previewed, or written when `commit`.
    Cursor {
        /// Of the digits' row, or the specials'.
        digit: bool,
        /// Its index.
        index: usize,
        /// Where, in percent.
        percent: f64,
        /// Written down, rather than previewed.
        commit: bool,
    },
    /// Export the batch: its place asked first.
    ExportBatch,
    /// Where to export, picked; `None` when cancelled.
    ExportTo(Option<PathBuf>),
    /// The export written, or why not.
    Exported(Result<(), String>),
    /// The history cleared.
    ClearHistory,
    /// A second of the clipboard's countdown `.0`.
    ClipboardTick(u64),
    /// The clipboard read at the end of countdown `.0`.
    ClipboardRead(u64, Option<String>),
    /// "Clipboard cleared" of countdown `.0` shown long enough.
    ClipboardHintDone(u64),
    /// Something done in the password's box, which does not change it.
    Output(Action),
}

impl fmt::Debug for PasswordMessage {
    /// Passwords and the clipboard are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Set(setting) => write!(f, "Set({setting:?})"),
            Self::Copy(copy) => write!(f, "Copy({copy:?})"),
            Self::ClipboardRead(timer, _) => write!(f, "ClipboardRead({timer}, ..)"),
            Self::Generate => f.write_str("Generate"),
            Self::Cursor {
                digit,
                index,
                commit,
                ..
            } => {
                write!(f, "Cursor({digit}, {index}, {commit})")
            }
            _ => f.write_str("PasswordMessage(..)"),
        }
    }
}

/// What an update asks of its tab.
#[derive(Debug)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This copied by its button; the clipboard's countdown `timer` started when it is a
    /// password and the clipboard is cleared after a delay.
    Copy(CopySlot, String, Option<u64>),
    /// The next second of countdown `.0`.
    Tick(u64),
    /// The clipboard read for countdown `.0`.
    ReadClipboard(u64),
    /// The clipboard emptied, then "Clipboard cleared" shown its time for countdown `.0`.
    ClearClipboard(u64),
    /// The export's save dialog.
    AskExport,
    /// This batch written at this path.
    Write(PathBuf, Zeroizing<String>),
}

impl Outcome {
    /// What tab `tab` runs for it, its dialogs over the window `main`.
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Password(message));
        match self {
            Self::Done | Self::Copy(..) => Task::none(),
            Self::Tick(timer) => tick(TICK, move || send(PasswordMessage::ClipboardTick(timer))),
            Self::ReadClipboard(timer) => iced::clipboard::read()
                .map(move |content| send(PasswordMessage::ClipboardRead(timer, content))),
            Self::ClearClipboard(timer) => Task::batch([
                iced::clipboard::write(String::new()),
                tick(CLEARED_SHOWN, move || {
                    send(PasswordMessage::ClipboardHintDone(timer))
                }),
            ]),
            Self::AskExport => key_parts::ask_save(
                main,
                fl!("ui-tool-password-title"),
                export_file_name(),
                fl!("ui-tool-password-text-filter"),
                EXPORT_EXTENSIONS,
                move |path| send(PasswordMessage::ExportTo(path)),
            ),
            Self::Write(path, text) => key_parts::off_thread(
                move || {
                    private_file::write_private(&path, text.as_bytes())
                        .map_err(|error| error.to_string())
                },
                move |written| {
                    send(PasswordMessage::Exported(
                        written.unwrap_or_else(|| Err(String::new())),
                    ))
                },
            ),
        }
    }
}

/// `message` after `duration`, its timer made in the runtime that runs it.
fn tick(duration: Duration, message: impl FnOnce() -> Message + Send + 'static) -> Task<Message> {
    Task::perform(
        async move { tokio::time::sleep(duration).await },
        move |()| message(),
    )
}

/// The export's file name, as the C#'s `passwords_{now:yyyyMMdd_HHmmss}.txt`.
fn export_file_name() -> String {
    let now = chrono::Local::now();
    format!(
        "{EXPORT_PREFIX}{}.{EXPORT_EXTENSION}",
        now.format("%Y%m%d_%H%M%S")
    )
}

/// The clipboard's countdown, as the C# `ShowClipboardClearHint` and
/// `StartClipboardClearTimer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Countdown {
    /// No password waits to be cleared: the keyboard hint.
    Idle,
    /// Seconds left before the clipboard is cleared.
    Running(u32),
    /// Just cleared.
    Cleared,
}

/// A prompt shown in the page, as the C# dialogs.
#[derive(Clone, PartialEq, Eq)]
enum Prompt {
    /// The name of a preset to save, as the C# `InputDialog`.
    PresetName(String),
    /// The deletion of this saved preset, as the C# `ShowConfirmAsync`.
    Delete(String),
}

/// The Password Generator's tab state.
pub struct PasswordPane {
    engine: PasswordGenerator,
    advanced: bool,
    preset: PresetEntry,
    /// The settings' changes the preset list last saw: a change takes it back to Custom.
    seen_changes: u64,
    prompt: Option<Prompt>,
    export_failed: Option<String>,
    output: Content,
    /// The countdown running, and the password it clears, to know it on the clipboard.
    timer: u64,
    countdown: Countdown,
    copied: Option<Zeroizing<String>>,
}

impl fmt::Debug for PasswordPane {
    /// The passwords are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasswordPane")
            .field("engine", &self.engine)
            .field("preset", &matches!(self.preset, PresetEntry::Custom))
            .field("countdown", &self.countdown)
            .finish_non_exhaustive()
    }
}

/// `value` as a slider's.
fn slider_value(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A slider's value as a count.
fn count(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

impl PasswordPane {
    /// A new tab, as the C# `Initialize`: its presets beside the profiles file
    /// `profiles_file`, its passphrase language the interface's.
    #[must_use]
    pub fn new(profiles_file: Option<&std::path::Path>) -> Self {
        let store = profiles_file.map(password_presets::presets_path);
        let engine = PasswordGenerator::new(store, crate::i18n::current().code());
        let output = Content::with_text(engine.password());
        let seen_changes = engine.settings_changes();
        Self {
            engine,
            advanced: false,
            preset: PresetEntry::Custom,
            seen_changes,
            prompt: None,
            export_failed: None,
            output,
            timer: 0,
            countdown: Countdown::Idle,
            copied: None,
        }
    }

    /// The engine, for the tests.
    #[cfg(test)]
    fn engine(&self) -> &PasswordGenerator {
        &self.engine
    }

    /// The tab closing: where the tool was left written down when it is to be remembered,
    /// as the C# `OnViewUnloaded`.
    pub fn closing(&mut self) {
        self.engine.persist_settings_if_remembering();
    }

    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: PasswordMessage) -> Outcome {
        let outcome = self.apply(message);
        // A setting moved by hand takes the list back to Custom, as the C# `OnSettingsChanged`.
        if self.engine.settings_changes() != self.seen_changes {
            self.seen_changes = self.engine.settings_changes();
            self.preset = PresetEntry::Custom;
        }
        self.refresh_output();
        outcome
    }

    fn apply(&mut self, message: PasswordMessage) -> Outcome {
        match message {
            PasswordMessage::Set(setting) => self.engine.set(setting),
            PasswordMessage::Generate => self.engine.generate(),
            PasswordMessage::ClearOutput => self.engine.clear_output(),
            PasswordMessage::Copy(copy) => return self.copy(copy),
            PasswordMessage::ToggleAdvanced => self.advanced = !self.advanced,
            PasswordMessage::Preset(entry) => self.apply_preset(entry),
            PasswordMessage::SavePreset => self.prompt = Some(Prompt::PresetName(String::new())),
            PasswordMessage::PresetName(typed) => {
                if let Some(Prompt::PresetName(name)) = &mut self.prompt {
                    *name = typed;
                }
            }
            PasswordMessage::PresetNameOk => {
                if let Some(Prompt::PresetName(name)) = self.prompt.take() {
                    let name = name.trim();
                    if !name.is_empty() {
                        self.engine.save_preset(name);
                    }
                }
            }
            PasswordMessage::PresetNameCancel | PasswordMessage::DeleteCancel => self.prompt = None,
            PasswordMessage::DeletePreset => {
                if let PresetEntry::Saved(name) = &self.preset {
                    self.prompt = Some(Prompt::Delete(name.clone()));
                }
            }
            PasswordMessage::DeleteConfirm => {
                if let Some(Prompt::Delete(name)) = self.prompt.take() {
                    self.engine.delete_preset(&name);
                    self.preset = PresetEntry::Custom;
                }
            }
            PasswordMessage::QuickLength(length) => self.engine.set(Setting::Length(length)),
            PasswordMessage::CaseBlockAdd => self.engine.add_case_block(),
            PasswordMessage::CaseBlockRemove => self.engine.remove_case_block(),
            PasswordMessage::CaseBlocksRandom => self.engine.randomize_case_blocks(),
            PasswordMessage::CaseBlocksAll(token) => self.engine.set_all_case_blocks(token),
            PasswordMessage::CaseBlockCycle(index) => self.engine.cycle_case_block(index),
            PasswordMessage::Distribute => self.engine.distribute_positions_evenly(),
            PasswordMessage::Cursor {
                digit,
                index,
                percent,
                commit,
            } => {
                // A preview moves the characters drawn; a drop writes the place, or draws
                // anew when the characters shown are no longer the ones drawn.
                if !self.engine.try_move_in_place(digit, index, percent, commit) && commit {
                    self.engine.move_position(digit, index, percent);
                }
            }
            PasswordMessage::ExportBatch => {
                if !self.engine.batch().is_empty() {
                    return Outcome::AskExport;
                }
            }
            PasswordMessage::ExportTo(Some(path)) => {
                self.export_failed = None;
                return Outcome::Write(path, self.engine.batch_text(NEW_LINE));
            }
            PasswordMessage::ExportTo(None) | PasswordMessage::Exported(Ok(())) => {}
            PasswordMessage::Exported(Err(error)) => self.export_failed = Some(error),
            PasswordMessage::ClearHistory => self.engine.clear_history(),
            PasswordMessage::ClipboardTick(timer) => {
                if timer == self.timer
                    && let Countdown::Running(left) = self.countdown
                {
                    if left > 1 {
                        self.countdown = Countdown::Running(left - 1);
                        return Outcome::Tick(timer);
                    }
                    return Outcome::ReadClipboard(timer);
                }
            }
            PasswordMessage::ClipboardRead(timer, content) => {
                if timer == self.timer {
                    let ours = content
                        .map(Zeroizing::new)
                        .is_some_and(|content| Some(&content) == self.copied.as_ref());
                    self.copied = None;
                    self.countdown = Countdown::Cleared;
                    if ours {
                        return Outcome::ClearClipboard(timer);
                    }
                    return Outcome::Tick(timer);
                }
            }
            PasswordMessage::ClipboardHintDone(timer) => {
                if timer == self.timer && self.countdown == Countdown::Cleared {
                    self.countdown = Countdown::Idle;
                }
            }
            PasswordMessage::Output(action) => super::read_only(&mut self.output, action),
        }
        Outcome::Done
    }

    /// A preset of the list applied, as the C# `OnPresetSelected`.
    fn apply_preset(&mut self, entry: PresetEntry) {
        match &entry {
            PresetEntry::Custom => {}
            PresetEntry::BuiltIn(preset) => preset.apply(&mut self.engine),
            PresetEntry::Saved(name) => {
                let saved = self
                    .engine
                    .presets_for_current_mode()
                    .into_iter()
                    .find(|preset| preset.name == *name);
                if let Some(saved) = saved {
                    self.engine.apply_preset(&saved);
                }
            }
        }
        // The preset's own writes are not edits.
        self.seen_changes = self.engine.settings_changes();
        self.preset = entry;
    }

    /// What a copy button copies, as the C# `OnCopyClick`, `OnHistoryCopyClick` and
    /// `CopyBatchText`: a password starts the clipboard's countdown when it is cleared.
    fn copy(&mut self, copy: PasswordCopy) -> Outcome {
        let (slot, content, secret) = match copy {
            PasswordCopy::Main => (
                CopySlot::PasswordMain,
                self.engine.password().to_owned(),
                true,
            ),
            PasswordCopy::Phonetic => (
                CopySlot::PasswordPhonetic,
                self.engine.phonetic().to_owned(),
                false,
            ),
            PasswordCopy::BatchAll => (
                CopySlot::PasswordBatchAll,
                self.engine.batch_text(NEW_LINE).to_string(),
                true,
            ),
            PasswordCopy::BatchRow(row) => (
                CopySlot::PasswordBatchRow(row),
                self.engine
                    .batch()
                    .get(row)
                    .map(|password| password.to_string())
                    .unwrap_or_default(),
                true,
            ),
            PasswordCopy::History(row) => (
                CopySlot::PasswordHistory(row),
                self.engine
                    .history()
                    .get(row)
                    .map(|password| password.to_string())
                    .unwrap_or_default(),
                true,
            ),
        };
        if content.is_empty() {
            return Outcome::Done;
        }
        let timer = (secret && self.engine.settings().clipboard_auto_clear).then(|| {
            self.timer += 1;
            self.copied = Some(Zeroizing::new(content.clone()));
            self.countdown = Countdown::Running(self.engine.clipboard_clear_seconds());
            self.timer
        });
        Outcome::Copy(slot, content, timer)
    }

    fn refresh_output(&mut self) {
        let shown = Zeroizing::new(super::box_text(&self.output));
        if shown.as_str() != self.engine.password() {
            self.output = Content::with_text(self.engine.password());
        }
    }

    /// The presets of the list for the mode shown, as the C# `RebuildPresetList`.
    fn preset_entries(&self, saved: &[String]) -> Vec<PresetEntry> {
        std::iter::once(PresetEntry::Custom)
            .chain(
                BuiltIn::of(self.engine.settings().mode)
                    .iter()
                    .map(|preset| PresetEntry::BuiltIn(*preset)),
            )
            .chain(saved.iter().map(|name| PresetEntry::Saved(name.clone())))
            .collect()
    }

    /// The tool's page, as the C# `PasswordGeneratorView.xaml`.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Password(message));
        let mut page = column![self.top(send)]
            .push(self.advanced.then(|| self.advanced_panel(send)))
            .push(self.prompt_panel(send))
            .push(self.output_panel(send, state))
            .push(self.strength_panel())
            .push(self.mode_panel(send))
            .push(
                self.engine
                    .show_case_blocks()
                    .then(|| self.case_blocks_panel(send)),
            )
            .push(
                self.engine
                    .show_placement_bar()
                    .then(|| self.placement_panel(send)),
            )
            .push(self.batch_count(send))
            .spacing(spacing::MD);
        page = page.push(self.details(send, state));
        super::content_column(page)
    }

    /// Row 0: the mode, the presets, Save and Delete, Advanced; the minimum strength; the
    /// presets saved elsewhere; the mode's description.
    fn top<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let settings = self.engine.settings();
        // The engine's store is read through a shared reference in the view: the saved
        // presets are the snapshot the pane keeps up to date.
        let saved = self.saved_names();
        let mode = pick_list(
            GeneratorMode::ALL.map(ModeChoice),
            Some(ModeChoice(settings.mode)),
            move |choice| send(PasswordMessage::Set(Setting::Mode(choice.0))),
        )
        .style(styles::pick_list)
        .menu_style(styles::menu)
        .text_size(font_size::BODY);
        let presets = tooltip(
            pick_list(
                self.preset_entries(&saved),
                Some(self.preset.clone()),
                move |entry| send(PasswordMessage::Preset(entry)),
            )
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .text_size(font_size::BODY),
            text(fl!("ui-tool-password-preset-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        let save = tooltip(
            super::action_button(
                fl!("ui-tool-password-save-preset"),
                false,
                Some(send(PasswordMessage::SavePreset)),
            ),
            text(fl!("ui-tool-password-save-preset-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        let delete = tooltip(
            super::action_button(
                fl!("ui-tool-password-delete-preset"),
                false,
                matches!(self.preset, PresetEntry::Saved(_))
                    .then_some(send(PasswordMessage::DeletePreset)),
            ),
            text(fl!("ui-tool-password-delete-preset-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        let advanced = tooltip(
            super::action_button(
                fl!("ui-tool-password-advanced"),
                self.advanced,
                Some(send(PasswordMessage::ToggleAdvanced)),
            ),
            text(fl!("ui-tool-password-advanced-tooltip")).size(font_size::CAPTION),
            tooltip::Position::Bottom,
        )
        .style(container::rounded_box);
        let first = row![
            super::field_label(fl!("ui-tool-password-mode")),
            mode,
            super::field_label(fl!("ui-tool-password-preset-label")),
            presets,
            save,
            delete,
            iced::widget::space::horizontal(),
            advanced,
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
        let floor = row![
            super::field_label(fl!("ui-tool-password-entropy-floor")),
            pick_list(
                (0..ENTROPY_FLOOR_CHOICES.len())
                    .map(FloorChoice)
                    .collect::<Vec<_>>(),
                Some(FloorChoice(settings.entropy_floor)),
                move |choice| send(PasswordMessage::Set(Setting::EntropyFloor(choice.0))),
            )
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .text_size(font_size::BODY),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
        let elsewhere = self.saved_elsewhere();
        let description = match settings.mode {
            GeneratorMode::Random => fl!("ui-tool-password-mode-random-desc"),
            GeneratorMode::Syllable => fl!("ui-tool-password-mode-syllable-desc"),
            GeneratorMode::Passphrase => fl!("ui-tool-password-mode-passphrase-desc"),
            GeneratorMode::Leet => fl!("ui-tool-password-mode-leet-desc"),
        };
        column![first, floor]
            .push((elsewhere > 0).then(|| {
                key_parts::hint(fl!("ui-tool-password-saved-elsewhere", count = elsewhere))
            }))
            .push(key_parts::hint(description))
            .spacing(spacing::SM)
            .into()
    }

    /// The names of the presets saved for the mode shown.
    fn saved_names(&self) -> Vec<String> {
        self.engine
            .presets_for_current_mode()
            .into_iter()
            .map(|preset| preset.name)
            .collect()
    }

    /// How many presets are saved in other modes, as the C# `SavedPresetsElsewhereText`.
    fn saved_elsewhere(&self) -> usize {
        self.engine
            .preset_count()
            .saturating_sub(self.engine.presets_for_current_mode().len())
    }

    /// The advanced options, as the C# `AdvancedPopup`: shown under the row rather than in a
    /// popup.
    fn advanced_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let settings = self.engine.settings();
        let check = |checked: bool, label: String, set: fn(bool) -> Setting| {
            checkbox(checked)
                .label(label)
                .on_toggle(move |on| send(PasswordMessage::Set(set(on))))
                .style(styles::checkbox)
                .text_size(font_size::BODY)
        };
        let mut panel = Column::new();
        if settings.mode == GeneratorMode::Random {
            panel = panel.push(check(
                settings.exclude_ambiguous,
                fl!("ui-tool-password-exclude-ambiguous"),
                Setting::ExcludeAmbiguous,
            ));
        }
        if self.engine.has_active_specials() {
            panel = panel.push(check(
                settings.cli_safe,
                fl!("ui-tool-password-cli-safe"),
                Setting::CliSafe,
            ));
        }
        if self.engine.show_layout_safe() {
            panel = panel.push(check(
                settings.layout_safe,
                fl!("ui-tool-password-layout-safe"),
                Setting::LayoutSafe,
            ));
        }
        panel = panel
            .push(check(
                settings.remember_settings,
                fl!("ui-tool-password-remember-settings"),
                Setting::RememberSettings,
            ))
            .push(key_parts::hint(fl!(
                "ui-tool-password-remember-settings-note"
            )));
        if self.engine.has_active_specials() {
            panel = panel.push(key_parts::labeled(
                fl!("ui-tool-password-custom-specials"),
                text_input("", &settings.custom_specials)
                    .font(super::BOX_FONT)
                    .size(font_size::BODY)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| {
                        let typed: String = typed.chars().take(SPECIALS_MAX).collect();
                        send(PasswordMessage::Set(Setting::CustomSpecials(typed)))
                    }),
            ));
            if let Some(notice) = self.engine.specials_notice() {
                panel = panel.push(key_parts::hint(match notice {
                    SpecialsNotice::NoneUsable => fl!("ui-tool-password-specials-none-usable"),
                    SpecialsNotice::Usable(usable) => {
                        fl!("ui-tool-password-specials-usable", specials = usable)
                    }
                }));
            }
        }
        let mut clipboard = row![check(
            settings.clipboard_auto_clear,
            fl!("ui-tool-password-clipboard-auto-clear"),
            Setting::ClipboardAutoClear,
        )]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
        if settings.clipboard_auto_clear {
            clipboard = clipboard.push(
                pick_list(
                    (0..CLIPBOARD_CLEAR_CHOICES.len())
                        .map(DelayChoice)
                        .collect::<Vec<_>>(),
                    Some(DelayChoice(settings.clipboard_clear_index)),
                    move |choice| {
                        send(PasswordMessage::Set(Setting::ClipboardClearIndex(choice.0)))
                    },
                )
                .style(styles::pick_list)
                .menu_style(styles::menu)
                .text_size(font_size::BODY),
            );
        }
        key_parts::card(panel.push(clipboard))
    }

    /// The prompt shown, as the C#'s preset name dialog and deletion confirmation.
    fn prompt_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Option<Element<'a, Message>> {
        let buttons = |ok: PasswordMessage, cancel: PasswordMessage| {
            row![
                super::action_button(fl!("ui-dialog-ok-button"), true, Some(send(ok))),
                super::action_button(fl!("ui-dialog-cancel-button"), false, Some(send(cancel))),
            ]
            .spacing(spacing::SM)
        };
        Some(match self.prompt.as_ref()? {
            Prompt::PresetName(name) => key_parts::card(key_parts::stack([
                key_parts::section_title(fl!("ui-tool-password-save-preset")),
                key_parts::hint(fl!("ui-tool-password-save-preset-prompt")),
                text_input("", name)
                    .size(font_size::BODY_LARGE)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| send(PasswordMessage::PresetName(typed)))
                    .on_submit(send(PasswordMessage::PresetNameOk))
                    .into(),
                buttons(
                    PasswordMessage::PresetNameOk,
                    PasswordMessage::PresetNameCancel,
                )
                .into(),
            ])),
            Prompt::Delete(name) => key_parts::card(key_parts::stack([
                key_parts::section_title(fl!("ui-tool-password-delete-preset-title")),
                key_parts::hint(fl!(
                    "ui-tool-password-delete-preset-confirm",
                    name = name.as_str()
                )),
                buttons(
                    PasswordMessage::DeleteConfirm,
                    PasswordMessage::DeleteCancel,
                )
                .into(),
            ])),
        })
    }

    /// Row 1: the password, Generate and Copy, and the keyboard hint or the clipboard's
    /// countdown.
    fn output_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
        state: &ToolPane,
    ) -> Element<'a, Message> {
        let output = super::text_box(&self.output, None)
            .size(font_size::LARGE)
            .height(OUTPUT_HEIGHT)
            .on_action(move |action| send(PasswordMessage::Output(action)));
        let buttons = column![
            tooltip(
                super::action_button(
                    fl!("ui-tool-password-generate"),
                    true,
                    Some(send(PasswordMessage::Generate)),
                ),
                text(fl!("ui-tool-password-generate-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Left,
            )
            .style(container::rounded_box),
            super::copy_button(
                fl!("ui-tool-password-copy"),
                state.copied(CopySlot::PasswordMain),
                send(PasswordMessage::Copy(PasswordCopy::Main)),
                super::COPY_PADDING,
            ),
        ]
        .spacing(spacing::XS);
        let hint = match self.countdown {
            Countdown::Idle => fl!("ui-tool-password-keyboard-hint"),
            Countdown::Running(seconds) => {
                fl!("ui-tool-password-clipboard-clear-hint", seconds = seconds)
            }
            Countdown::Cleared => fl!("ui-tool-password-clipboard-cleared"),
        };
        column![
            container(
                row![output, buttons]
                    .spacing(spacing::SM)
                    .align_y(Alignment::Center)
            )
            .padding(spacing::SM)
            .style(styles::card),
            container(key_parts::hint(hint)).align_right(Length::Fill),
        ]
        .spacing(spacing::XS)
        .into()
    }

    /// Row 2: the strength bar and its word, the crack time, the issues, the notices.
    fn strength_panel(&self) -> Option<Element<'_, Message>> {
        let strength = self.engine.strength()?;
        let level = strength.level;
        let word = match level {
            StrengthLevel::Critical => fl!("ui-tool-password-strength-critical"),
            StrengthLevel::Weak => fl!("ui-tool-password-strength-weak"),
            StrengthLevel::Fair => fl!("ui-tool-password-strength-fair"),
            StrengthLevel::Good => fl!("ui-tool-password-strength-good"),
            StrengthLevel::Strong => fl!("ui-tool-password-strength-strong"),
        };
        let bits = format!("{:.0}", strength.bits.round());
        let bar = progress_bar(0.0..=1.0, strength.fill)
            .girth(STRENGTH_HEIGHT)
            .style(move |theme: &Theme| progress_bar::Style {
                bar: iced::Background::Color(strength_color(level, theme)),
                ..progress_bar::primary(theme)
            });
        let mut panel = column![
            bar,
            text(format!("{word} ({bits} {})", fl!("ui-tool-password-bits")))
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .style(move |theme: &Theme| text::Style {
                    color: Some(strength_color(level, theme)),
                }),
        ]
        .spacing(spacing::XS);
        if let Some(crack) = self.engine.crack_time() {
            panel = panel
                .push(key_parts::hint(fl!(
                    "ui-tool-password-crack-time",
                    time = crack_text(crack)
                )))
                .push(key_parts::hint(fl!(
                    "ui-tool-password-crack-assumption",
                    rate = rules::guess_rate_text()
                )));
        }
        let issues: Vec<String> = self.engine.issues().iter().map(issue_text).collect();
        if !issues.is_empty() {
            panel = panel.push(crypto_parts::said(
                issues.join(ISSUE_SEPARATOR),
                Tone::Error,
                font_size::CAPTION,
                false,
            ));
        }
        if let Some(found) = self.engine.floor_search_notice() {
            panel = panel.push(key_parts::hint(floor_search_text(found)));
        }
        let notices: Vec<String> = self.engine.notices().iter().map(notice_text).collect();
        if !notices.is_empty() {
            panel = panel.push(key_parts::hint(notices.join(NOTICE_SEPARATOR)));
        }
        Some(panel.into())
    }

    /// A slider of `label` over `range`, its value shown, as the C#'s label, value and
    /// `Slider` rows.
    fn slider_row<'a>(
        label: String,
        range: std::ops::RangeInclusive<u32>,
        value: usize,
        step: u32,
        on_change: impl Fn(u32) -> Message + 'a,
    ) -> Element<'a, Message> {
        row![
            text(label)
                .size(font_size::BODY)
                .width(key_parts::LABEL_WIDTH),
            text(value.to_string())
                .size(font_size::BODY)
                .font(styles::SEMIBOLD)
                .width(VALUE_WIDTH),
            slider(range, slider_value(value), on_change).step(step),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
        .into()
    }

    /// A box of `choices` after its `label`.
    fn choice_row<'a, T>(
        label: String,
        choices: Vec<T>,
        chosen: T,
        on_select: impl Fn(T) -> Message + 'a,
    ) -> Element<'a, Message>
    where
        T: fmt::Display + Clone + PartialEq + 'a,
    {
        key_parts::labeled(
            label,
            pick_list(choices, Some(chosen), on_select)
                .style(styles::pick_list)
                .menu_style(styles::menu)
                .text_size(font_size::BODY),
        )
    }

    /// Row 3: the controls of the mode shown, as the C# `PanelRandom`, `PanelSyllable`,
    /// `PanelPassphrase` and `PanelLeet`.
    fn mode_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let content = match self.engine.settings().mode {
            GeneratorMode::Random => self.random_panel(send),
            GeneratorMode::Syllable => self.syllable_panel(send),
            GeneratorMode::Passphrase => self.passphrase_panel(send),
            GeneratorMode::Leet => self.leet_panel(send),
        };
        key_parts::card(content)
    }

    fn random_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Column<'a, Message> {
        let settings = self.engine.settings();
        let quick = QUICK_LENGTHS.iter().fold(
            row![key_parts::hint(fl!("ui-tool-password-quick-length"))]
                .spacing(spacing::XS)
                .align_y(Alignment::Center),
            |quick, length| {
                quick.push(
                    button(text(length.to_string()).size(font_size::CAPTION))
                        .padding(super::COPY_PADDING)
                        .style(if settings.length == *length {
                            styles::primary
                        } else {
                            styles::secondary
                        })
                        .on_press(send(PasswordMessage::QuickLength(*length))),
                )
            },
        );
        let check = |checked: bool, label: String, set: fn(bool) -> Setting| {
            checkbox(checked)
                .label(label)
                .on_toggle(move |on| send(PasswordMessage::Set(set(on))))
                .style(styles::checkbox)
                .text_size(font_size::BODY)
        };
        key_parts::stack([
            quick.into(),
            Self::slider_row(
                fl!("ui-tool-password-length"),
                slider_value(MINIMUM_LENGTH)..=slider_value(MAXIMUM_LENGTH),
                settings.length,
                1,
                move |value| send(PasswordMessage::Set(Setting::Length(count(value)))),
            ),
            row![
                check(
                    settings.include_uppercase,
                    fl!("ui-tool-password-uppercase"),
                    Setting::IncludeUppercase
                ),
                check(
                    settings.include_lowercase,
                    fl!("ui-tool-password-lowercase"),
                    Setting::IncludeLowercase
                ),
            ]
            .spacing(spacing::LG)
            .into(),
            row![
                check(
                    settings.include_digits,
                    fl!("ui-tool-password-digits"),
                    Setting::IncludeDigits
                ),
                check(
                    settings.include_symbols,
                    fl!("ui-tool-password-symbols"),
                    Setting::IncludeSymbols
                ),
            ]
            .spacing(spacing::LG)
            .into(),
        ])
    }

    /// The case box and the extras' sliders and placement shared by three modes.
    #[expect(clippy::too_many_arguments, reason = "the three modes' own settings")]
    fn extras_rows<'a>(
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
        digits: usize,
        specials: usize,
        maximum: usize,
        placement: Option<Placement>,
        set_digits: fn(usize) -> Setting,
        set_specials: fn(usize) -> Setting,
        set_placement: fn(Placement) -> Setting,
    ) -> Vec<Element<'a, Message>> {
        let mut rows = vec![
            Self::slider_row(
                fl!("ui-tool-password-digits"),
                0..=slider_value(maximum),
                digits,
                1,
                move |value| send(PasswordMessage::Set(set_digits(count(value)))),
            ),
            Self::slider_row(
                fl!("ui-tool-password-symbols"),
                0..=slider_value(maximum),
                specials,
                1,
                move |value| send(PasswordMessage::Set(set_specials(count(value)))),
            ),
        ];
        if let Some(placement) = placement {
            rows.push(Self::choice_row(
                fl!("ui-tool-password-placement"),
                Placement::ALL.map(PlacementChoice).to_vec(),
                PlacementChoice(placement),
                move |choice| send(PasswordMessage::Set(set_placement(choice.0))),
            ));
        }
        rows
    }

    /// A case box setting `set`.
    fn case_row<'a>(
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
        case: CaseMode,
        set: fn(CaseMode) -> Setting,
    ) -> Element<'a, Message> {
        Self::choice_row(
            fl!("ui-tool-password-case"),
            CaseMode::ALL.map(CaseChoice).to_vec(),
            CaseChoice(case),
            move |choice| send(PasswordMessage::Set(set(choice.0))),
        )
    }

    /// A short text box of `value`, at most `limit` characters.
    fn short_field<'a>(
        label: String,
        value: &'a str,
        limit: usize,
        on_input: impl Fn(String) -> Message + 'a,
    ) -> Element<'a, Message> {
        key_parts::labeled(
            label,
            text_input("", value)
                .font(super::BOX_FONT)
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .width(SHORT_FIELD_WIDTH)
                .style(styles::text_input)
                .on_input(move |typed| on_input(typed.chars().take(limit).collect())),
        )
    }

    fn syllable_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Column<'a, Message> {
        let settings = self.engine.settings();
        let step = if settings.syllable_cvc {
            1
        } else {
            slider_value(SYLLABLE_LENGTH_STEP)
        };
        let mut rows = vec![
            Self::slider_row(
                fl!("ui-tool-password-length"),
                slider_value(MINIMUM_SYLLABLE_LENGTH)..=slider_value(MAXIMUM_SYLLABLE_LENGTH),
                settings.syllable_length,
                step,
                move |value| send(PasswordMessage::Set(Setting::SyllableLength(count(value)))),
            ),
            key_parts::hint(fl!(
                "ui-tool-password-total-length",
                count = self.engine.syllable_total_length()
            )),
            key_parts::hint(if settings.syllable_cvc {
                fl!("ui-tool-password-syl-step-note-cvc")
            } else {
                fl!("ui-tool-password-syl-step-note")
            }),
            Self::short_field(
                fl!("ui-tool-password-separator"),
                &settings.syllable_separator,
                SEPARATOR_MAX,
                move |typed| send(PasswordMessage::Set(Setting::SyllableSeparator(typed))),
            ),
            Self::case_row(send, settings.syllable_case, Setting::SyllableCase),
            checkbox(settings.syllable_cvc)
                .label(fl!("ui-tool-password-syl-cvc"))
                .on_toggle(move |on| send(PasswordMessage::Set(Setting::SyllableCvc(on))))
                .style(styles::checkbox)
                .text_size(font_size::BODY)
                .into(),
            key_parts::hint(fl!("ui-tool-password-syl-cvc-hint")),
        ];
        let placement = (settings.syllable_digits > 0 || settings.syllable_specials > 0)
            .then_some(settings.syllable_placement);
        rows.extend(Self::extras_rows(
            send,
            settings.syllable_digits,
            settings.syllable_specials,
            MAXIMUM_SYLLABLE_EXTRAS,
            placement,
            Setting::SyllableDigits,
            Setting::SyllableSpecials,
            Setting::SyllablePlacement,
        ));
        key_parts::stack(rows)
    }

    /// The language box and what its list is worth, shared by the passphrase and leet modes.
    fn language_row<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let mut language = row![Self::choice_row(
            fl!("ui-tool-password-language"),
            (0..PASSPHRASE_LANGUAGES.len())
                .map(LanguageChoice)
                .collect(),
            LanguageChoice(self.engine.settings().passphrase_language),
            move |choice| send(PasswordMessage::Set(Setting::PassphraseLanguage(choice.0))),
        )]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
        if let Some((words, bits)) = self.engine.word_list_summary() {
            language = language.push(key_parts::hint(fl!(
                "ui-tool-password-word-list-size",
                words = words,
                bits = format!("{bits:.1}")
            )));
        }
        language.into()
    }

    fn passphrase_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Column<'a, Message> {
        let settings = self.engine.settings();
        let mut rows = vec![
            Self::slider_row(
                fl!("ui-tool-password-word-count"),
                slider_value(MINIMUM_PASSPHRASE_WORDS)..=slider_value(MAXIMUM_PASSPHRASE_WORDS),
                settings.passphrase_word_count,
                1,
                move |value| {
                    send(PasswordMessage::Set(Setting::PassphraseWordCount(count(
                        value,
                    ))))
                },
            ),
            Self::short_field(
                fl!("ui-tool-password-separator"),
                &settings.passphrase_separator,
                SEPARATOR_MAX,
                move |typed| send(PasswordMessage::Set(Setting::PassphraseSeparator(typed))),
            ),
            self.language_row(send),
            Self::case_row(send, settings.passphrase_case, Setting::PassphraseCase),
        ];
        let placement = (settings.passphrase_digits > 0 || settings.passphrase_specials > 0)
            .then_some(settings.passphrase_placement);
        rows.extend(Self::extras_rows(
            send,
            settings.passphrase_digits,
            settings.passphrase_specials,
            MAXIMUM_LEET_EXTRAS,
            placement,
            Setting::PassphraseDigits,
            Setting::PassphraseSpecials,
            Setting::PassphrasePlacement,
        ));
        key_parts::stack(rows)
    }

    fn leet_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Column<'a, Message> {
        let settings = self.engine.settings();
        let mut rows = vec![
            checkbox(settings.leet_random_word)
                .label(fl!("ui-tool-password-leet-random-word"))
                .on_toggle(move |on| send(PasswordMessage::Set(Setting::LeetRandomWord(on))))
                .style(styles::checkbox)
                .text_size(font_size::BODY)
                .into(),
        ];
        if settings.leet_random_word {
            rows.push(self.language_row(send));
        } else {
            rows.push(key_parts::labeled(
                fl!("ui-tool-password-leet-word"),
                text_input("", &settings.leet_base_word)
                    .size(font_size::BODY)
                    .padding(super::INPUT_PADDING)
                    .style(styles::text_input)
                    .on_input(move |typed| {
                        let typed: String = typed.chars().take(LEET_WORD_MAX).collect();
                        send(PasswordMessage::Set(Setting::LeetBaseWord(typed)))
                    }),
            ));
        }
        rows.push(
            checkbox(settings.leet_full_substitution)
                .label(fl!("ui-tool-password-leet-full-substitution"))
                .on_toggle(move |on| send(PasswordMessage::Set(Setting::LeetFullSubstitution(on))))
                .style(styles::checkbox)
                .text_size(font_size::BODY)
                .into(),
        );
        rows.push(Self::case_row(send, settings.leet_case, Setting::LeetCase));
        let placement = self
            .engine
            .show_leet_placement()
            .then_some(settings.leet_placement);
        rows.extend(Self::extras_rows(
            send,
            settings.leet_digits,
            settings.leet_specials,
            MAXIMUM_LEET_EXTRAS,
            placement,
            Setting::LeetDigits,
            Setting::LeetSpecials,
            Setting::LeetPlacement,
        ));
        if !self.engine.leet_word_source().is_empty() {
            rows.push(key_parts::labeled(
                fl!("ui-tool-password-leet-word-source"),
                text(self.engine.leet_word_source())
                    .size(font_size::BODY)
                    .font(super::BOX_FONT),
            ));
        }
        key_parts::stack(rows)
    }

    /// Row 4: the case blocks, as the C# `PanelCaseBlocks`.
    fn case_blocks_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let settings = self.engine.settings();
        let small = |label: String,
                     message: PasswordMessage,
                     tip: Option<String>|
         -> Element<'a, Message> {
            let pressed = button(text(label).size(font_size::BODY).font(super::BOX_FONT))
                .padding(super::COPY_PADDING)
                .style(styles::secondary)
                .on_press(send(message));
            match tip {
                Some(tip) => tooltip(
                    pressed,
                    text(tip).size(font_size::CAPTION),
                    tooltip::Position::Bottom,
                )
                .style(container::rounded_box)
                .into(),
                None => pressed.into(),
            }
        };
        let blocks = settings.case_blocks.chars().enumerate().fold(
            row![].spacing(spacing::XS),
            |blocks, (index, token)| {
                blocks.push(
                    tooltip(
                        button(
                            container(
                                text(token.to_string())
                                    .size(font_size::BODY_LARGE)
                                    .font(super::BOX_FONT),
                            )
                            .center_x(Length::Fill),
                        )
                        .width(BLOCK_WIDTH)
                        .padding(super::COPY_PADDING)
                        .style(styles::secondary)
                        .on_press(send(PasswordMessage::CaseBlockCycle(index))),
                        text(fl!("ui-tool-password-blocks-hint")).size(font_size::CAPTION),
                        tooltip::Position::Bottom,
                    )
                    .style(container::rounded_box),
                )
            },
        );
        let editor = row![
            blocks,
            small(
                "-".to_owned(),
                PasswordMessage::CaseBlockRemove,
                Some(fl!("ui-tool-password-blocks-remove"))
            ),
            small(
                "+".to_owned(),
                PasswordMessage::CaseBlockAdd,
                Some(fl!("ui-tool-password-blocks-add"))
            ),
            small(
                fl!("ui-tool-password-blocks-random"),
                PasswordMessage::CaseBlocksRandom,
                None
            ),
            small(
                std::iter::repeat_n(CASE_BLOCK_TOKENS[0], 3).collect(),
                PasswordMessage::CaseBlocksAll(CASE_BLOCK_TOKENS[0]),
                Some(fl!("ui-tool-password-blocks-all-upper")),
            ),
            small(
                std::iter::repeat_n(CASE_BLOCK_TOKENS[1], 3).collect(),
                PasswordMessage::CaseBlocksAll(CASE_BLOCK_TOKENS[1]),
                Some(fl!("ui-tool-password-blocks-all-lower")),
            ),
            small(
                std::iter::repeat_n(CASE_BLOCK_TOKENS[2], 3).collect(),
                PasswordMessage::CaseBlocksAll(CASE_BLOCK_TOKENS[2]),
                Some(fl!("ui-tool-password-blocks-all-title")),
            ),
        ]
        .spacing(spacing::XS)
        .align_y(Alignment::Center);
        let mut panel = key_parts::stack([
            row![
                super::field_label(fl!("ui-tool-password-blocks")),
                iced::widget::space::horizontal(),
                key_parts::hint(fl!("ui-tool-password-blocks-hint")),
            ]
            .into(),
            editor.into(),
        ]);
        if self.engine.show_case_blocks_auto_sync() {
            let label = if settings.mode == GeneratorMode::Passphrase {
                fl!("ui-tool-password-blocks-auto-sync-words")
            } else {
                fl!("ui-tool-password-blocks-auto-sync")
            };
            panel = panel.push(
                checkbox(settings.case_blocks_auto_sync)
                    .label(label)
                    .on_toggle(move |on| {
                        send(PasswordMessage::Set(Setting::CaseBlocksAutoSync(on)))
                    })
                    .style(styles::checkbox)
                    .text_size(font_size::BODY),
            );
        }
        key_parts::card(panel)
    }

    /// The placement bar, as the C# `PanelPlacementBar`: a row of cursors per kind of
    /// character, and Spread evenly.
    fn placement_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        let settings = self.engine.settings();
        let length = rules::utf16_length(self.engine.password());
        let (digits, specials) = (
            self.engine.current_digit_count(),
            self.engine.current_special_count(),
        );
        let track = |digit: bool, label: String, positions: &str| -> Element<'a, Message> {
            let positions = rules::parse_positions(positions);
            row![
                text(label)
                    .size(font_size::BODY)
                    .width(key_parts::LABEL_WIDTH),
                canvas(PlacementTrack::new(
                    positions,
                    rules::slot_count(length, digits, specials, digit),
                    move |index, percent, commit| {
                        send(PasswordMessage::Cursor {
                            digit,
                            index,
                            percent,
                            commit,
                        })
                    },
                ))
                .width(Length::Fill)
                .height(TRACK_HEIGHT),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center)
            .into()
        };
        let mut panel = key_parts::stack([row![
            super::field_label(fl!("ui-tool-password-placement-bar")),
            iced::widget::space::horizontal(),
            key_parts::hint(fl!("ui-tool-password-placement-bar-hint")),
        ]
        .into()]);
        if digits > 0 {
            panel = panel.push(track(
                true,
                fl!("ui-tool-password-digits"),
                &settings.digit_positions,
            ));
        }
        if specials > 0 {
            panel = panel.push(track(
                false,
                fl!("ui-tool-password-symbols"),
                &settings.special_positions,
            ));
        }
        key_parts::card(panel.push(super::action_button(
            fl!("ui-tool-password-placement-distribute"),
            false,
            Some(send(PasswordMessage::Distribute)),
        )))
    }

    /// Row 5: how many passwords one click makes.
    fn batch_count<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
    ) -> Element<'a, Message> {
        Self::slider_row(
            fl!("ui-tool-password-batch-count"),
            slider_value(MINIMUM_BATCH_COUNT)..=slider_value(MAXIMUM_BATCH_COUNT),
            self.engine.settings().batch_count,
            1,
            move |value| send(PasswordMessage::Set(Setting::BatchCount(count(value)))),
        )
    }

    /// The details, as the C# right column: the structure, the phonetic reading, the batch,
    /// the history.
    fn details<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
        state: &ToolPane,
    ) -> Element<'a, Message> {
        let mut details = Column::new().spacing(spacing::MD);
        if self.engine.settings().mode == GeneratorMode::Syllable
            && !self.engine.syllable_structure().is_empty()
        {
            details = details.push(key_parts::stack([
                super::field_label(fl!("ui-tool-password-syl-structure")),
                text(self.engine.syllable_structure())
                    .size(font_size::BODY)
                    .font(super::BOX_FONT)
                    .into(),
            ]));
        }
        if !self.engine.phonetic().is_empty() {
            details = details.push(key_parts::stack([
                row![
                    super::field_label(fl!("ui-tool-password-phonetic")),
                    iced::widget::space::horizontal(),
                    super::copy_button(
                        fl!("ui-tool-password-copy-phonetic"),
                        state.copied(CopySlot::PasswordPhonetic),
                        send(PasswordMessage::Copy(PasswordCopy::Phonetic)),
                        super::COPY_PADDING,
                    ),
                ]
                .align_y(Alignment::Center)
                .into(),
                text(self.engine.phonetic()).size(font_size::CAPTION).into(),
            ]));
        }
        if self.engine.show_batch() {
            details = details.push(self.batch_panel(send, state));
        }
        details.push(self.history_panel(send, state)).into()
    }

    /// The batch, as the C# `PanelBatch`.
    fn batch_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
        state: &ToolPane,
    ) -> Element<'a, Message> {
        let label = self.export_failed.as_ref().map_or_else(
            || fl!("ui-tool-password-batch"),
            |error| {
                fl!(
                    "ui-tool-password-batch-export-failed",
                    error = error.as_str()
                )
            },
        );
        let head = row![
            super::field_label(label),
            iced::widget::space::horizontal(),
            checkbox(self.engine.settings().mask_batch)
                .label(fl!("ui-tool-password-batch-mask"))
                .on_toggle(move |on| send(PasswordMessage::Set(Setting::MaskBatch(on))))
                .style(styles::checkbox)
                .text_size(font_size::CAPTION),
            super::copy_button(
                fl!("ui-tool-password-batch-copy-all"),
                state.copied(CopySlot::PasswordBatchAll),
                send(PasswordMessage::Copy(PasswordCopy::BatchAll)),
                super::COPY_PADDING,
            ),
            super::action_button(
                fl!("ui-tool-password-batch-export"),
                false,
                Some(send(PasswordMessage::ExportBatch)),
            ),
        ]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);
        let rows = self
            .engine
            .batch_rows()
            .into_iter()
            .enumerate()
            .map(|(index, shown)| {
                password_row(
                    shown.to_string(),
                    state.copied(CopySlot::PasswordBatchRow(index)),
                    send(PasswordMessage::Copy(PasswordCopy::BatchRow(index))),
                )
            });
        key_parts::card(key_parts::stack(std::iter::once(head.into()).chain(rows)))
    }

    /// The history, as the C# `HistoryList`.
    fn history_panel<'a>(
        &'a self,
        send: impl Fn(PasswordMessage) -> Message + Copy + 'a,
        state: &ToolPane,
    ) -> Element<'a, Message> {
        let head = row![
            text(fl!("ui-tool-password-history"))
                .size(font_size::CAPTION)
                .font(styles::SEMIBOLD),
            iced::widget::space::horizontal(),
            tooltip(
                super::action_button(
                    fl!("ui-tool-password-clear-history"),
                    false,
                    Some(send(PasswordMessage::ClearHistory)),
                ),
                text(fl!("ui-tool-password-clear-history-tooltip")).size(font_size::CAPTION),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box),
        ]
        .align_y(Alignment::Center);
        let history = self.engine.history();
        let rows: Vec<Element<'a, Message>> = if history.is_empty() {
            vec![key_parts::hint(fl!("ui-tool-password-history-empty"))]
        } else {
            history
                .iter()
                .enumerate()
                .map(|(index, password)| {
                    password_row(
                        password.to_string(),
                        state.copied(CopySlot::PasswordHistory(index)),
                        send(PasswordMessage::Copy(PasswordCopy::History(index))),
                    )
                })
                .collect()
        };
        key_parts::card(key_parts::stack(std::iter::once(head.into()).chain(rows)))
    }
}

/// A password of a list and its copy button, as the C# batch and history rows.
fn password_row<'a>(shown: String, copied: bool, copy: Message) -> Element<'a, Message> {
    row![
        text(shown)
            .size(font_size::CAPTION)
            .font(super::BOX_FONT)
            .width(Length::Fill),
        super::copy_button(
            fl!("ui-tool-password-copy"),
            copied,
            copy,
            super::COPY_PADDING
        ),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center)
    .into()
}

/// The strength's colour, as the C# `UpdateStrengthBarBrush`: error, warning, accent, info,
/// success.
fn strength_color(level: StrengthLevel, theme: &Theme) -> Color {
    let palette = theme.extended_palette();
    match level {
        StrengthLevel::Critical => palette.danger.base.color,
        StrengthLevel::Weak => palette.warning.base.color,
        StrengthLevel::Fair => palette.primary.base.color,
        StrengthLevel::Good => palette.primary.strong.color,
        StrengthLevel::Strong => palette.success.base.color,
    }
}

/// The crack time said, as the C# `UpdateCrackTimeEstimate`'s units.
fn crack_text(crack: CrackTime) -> String {
    match crack {
        CrackTime::Instant => fl!("ui-tool-password-crack-instant"),
        CrackTime::Seconds(count) => fl!("ui-tool-password-crack-seconds", count = count),
        CrackTime::Minutes(count) => fl!("ui-tool-password-crack-minutes", count = count),
        CrackTime::Hours(count) => fl!("ui-tool-password-crack-hours", count = count),
        CrackTime::Days(count) => fl!("ui-tool-password-crack-days", count = count),
        CrackTime::Years(count) => fl!("ui-tool-password-crack-years", count = count),
        CrackTime::Centuries(count) => fl!("ui-tool-password-crack-centuries", count = count),
        CrackTime::Forever => fl!("ui-tool-password-crack-forever"),
    }
}

/// An issue said, as the C# `UpdateIssuesList`'s keys.
fn issue_text(issue: &Issue) -> String {
    match *issue {
        Issue::TooShort => fl!("ui-tool-password-issue-too-short"),
        Issue::FloorUnreachable { floor, ceiling } => fl!(
            "ui-tool-password-issue-floor-unreachable",
            floor = floor,
            ceiling = ceiling
        ),
        Issue::ChosenWord => fl!("ui-tool-password-issue-chosen-word"),
        Issue::NoUpper => fl!("ui-tool-password-issue-no-upper"),
        Issue::NoLower => fl!("ui-tool-password-issue-no-lower"),
        Issue::NoDigit => fl!("ui-tool-password-issue-no-digit"),
        Issue::NoSpecial => fl!("ui-tool-password-issue-no-special"),
    }
}

/// A notice said, as the C# `FloorNoticeText`'s sentences.
fn notice_text(notice: &Notice) -> String {
    match notice {
        Notice::FloorRaised { chosen, used } => fl!(
            "ui-tool-password-floor-raised",
            chosen = chosen.as_str(),
            used = used.as_str()
        ),
        Notice::CountsCut { digits, specials } => fl!(
            "ui-tool-password-counts-cut",
            digits = digits,
            specials = specials
        ),
        Notice::ClassesNotPromised => fl!("ui-tool-password-classes-not-promised"),
    }
}

/// What asking for a minimum did, said, as the C# `FloorSearchNoticeText`.
fn floor_search_text(found: FloorSearchNotice) -> String {
    match found {
        FloorSearchNotice::Length { length, floor } => {
            fl!(
                "ui-tool-password-floor-set-length",
                length = length,
                floor = floor
            )
        }
        FloorSearchNotice::Syllable {
            length,
            digits,
            specials,
            floor,
        } => fl!(
            "ui-tool-password-floor-set-syllable",
            length = length,
            digits = digits,
            specials = specials,
            floor = floor
        ),
        FloorSearchNotice::Passphrase {
            words,
            digits,
            specials,
            floor,
        } => fl!(
            "ui-tool-password-floor-set-passphrase",
            words = words,
            digits = digits,
            specials = specials,
            floor = floor
        ),
        FloorSearchNotice::Leet {
            digits,
            specials,
            floor,
        } => fl!(
            "ui-tool-password-floor-set-leet",
            digits = digits,
            specials = specials,
            floor = floor
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane() -> PasswordPane {
        PasswordPane::new(None)
    }

    #[test]
    fn the_pane_shows_the_engines_password_and_copies_it() {
        let mut pane = pane();
        assert_eq!(
            super::super::box_text(&pane.output),
            pane.engine().password()
        );
        let Outcome::Copy(CopySlot::PasswordMain, copied, None) =
            pane.update(PasswordMessage::Copy(PasswordCopy::Main))
        else {
            panic!("copied without a countdown");
        };
        assert_eq!(copied, pane.engine().password());
        pane.update(PasswordMessage::Generate);
        assert_eq!(
            super::super::box_text(&pane.output),
            pane.engine().password()
        );
    }

    #[test]
    fn a_preset_applies_and_a_setting_moved_takes_the_list_back_to_custom() {
        let mut pane = pane();
        pane.update(PasswordMessage::Preset(PresetEntry::BuiltIn(BuiltIn::Pin4)));
        assert_eq!(pane.preset, PresetEntry::BuiltIn(BuiltIn::Pin4));
        assert_eq!(pane.engine().password().len(), 4);
        assert!(pane.engine().password().chars().all(|c| c.is_ascii_digit()));
        pane.update(PasswordMessage::Generate);
        assert_eq!(
            pane.preset,
            PresetEntry::BuiltIn(BuiltIn::Pin4),
            "a reroll is no change"
        );
        pane.update(PasswordMessage::QuickLength(12));
        assert_eq!(pane.preset, PresetEntry::Custom);
    }

    #[test]
    fn presets_are_saved_named_and_deleted_once_confirmed() {
        let dir = tempfile::tempdir().expect("dir");
        let mut pane = PasswordPane::new(Some(&dir.path().join("profiles.toml")));
        pane.update(PasswordMessage::SavePreset);
        pane.update(PasswordMessage::PresetName("  Mine ".to_owned()));
        pane.update(PasswordMessage::PresetNameOk);
        assert_eq!(pane.saved_names(), ["Mine"]);
        assert!(dir.path().join("password-presets.json").exists());
        pane.update(PasswordMessage::Preset(PresetEntry::Saved(
            "Mine".to_owned(),
        )));
        pane.update(PasswordMessage::DeletePreset);
        assert!(matches!(pane.prompt, Some(Prompt::Delete(_))));
        pane.update(PasswordMessage::DeleteCancel);
        assert_eq!(pane.saved_names(), ["Mine"]);
        pane.update(PasswordMessage::DeletePreset);
        pane.update(PasswordMessage::DeleteConfirm);
        assert!(pane.saved_names().is_empty());
        pane.update(PasswordMessage::Set(Setting::Mode(GeneratorMode::Syllable)));
        assert_eq!(pane.saved_elsewhere(), 0);
    }

    #[test]
    fn a_copied_password_is_cleared_from_the_clipboard_after_its_delay() {
        let mut pane = pane();
        pane.update(PasswordMessage::Set(Setting::ClipboardAutoClear(true)));
        pane.update(PasswordMessage::Set(Setting::ClipboardClearIndex(1)));
        let Outcome::Copy(_, copied, Some(timer)) =
            pane.update(PasswordMessage::Copy(PasswordCopy::Main))
        else {
            panic!("a countdown");
        };
        assert_eq!(pane.countdown, Countdown::Running(10));
        for _ in 0..9 {
            assert!(matches!(
                pane.update(PasswordMessage::ClipboardTick(timer)),
                Outcome::Tick(_)
            ));
        }
        assert!(matches!(
            pane.update(PasswordMessage::ClipboardTick(timer)),
            Outcome::ReadClipboard(_)
        ));
        assert!(matches!(
            pane.update(PasswordMessage::ClipboardRead(timer, Some(copied))),
            Outcome::ClearClipboard(_)
        ));
        assert_eq!(pane.countdown, Countdown::Cleared);
        pane.update(PasswordMessage::ClipboardHintDone(timer));
        assert_eq!(pane.countdown, Countdown::Idle);
        // Something else copied meanwhile is left where it is.
        let Outcome::Copy(_, _, Some(timer)) =
            pane.update(PasswordMessage::Copy(PasswordCopy::Main))
        else {
            panic!("a countdown");
        };
        assert!(matches!(
            pane.update(PasswordMessage::ClipboardRead(
                timer,
                Some("other".to_owned())
            )),
            Outcome::Tick(_)
        ));
    }

    #[test]
    fn a_cursor_dragged_previews_then_writes_its_place() {
        let mut pane = pane();
        for setting in [
            Setting::Mode(GeneratorMode::Syllable),
            Setting::SyllableDigits(1),
            Setting::SyllableSpecials(0),
            Setting::SyllablePlacement(Placement::Positions),
        ] {
            pane.update(PasswordMessage::Set(setting));
        }
        pane.update(PasswordMessage::Cursor {
            digit: true,
            index: 0,
            percent: 0.0,
            commit: true,
        });
        assert_eq!(pane.engine().settings().digit_positions, "0");
        assert!(
            pane.engine()
                .password()
                .starts_with(|c: char| c.is_ascii_digit())
        );
        assert_eq!(
            super::super::box_text(&pane.output),
            pane.engine().password()
        );
    }

    #[test]
    fn the_batch_exports_one_password_per_line() {
        let mut pane = pane();
        pane.update(PasswordMessage::Set(Setting::BatchCount(3)));
        assert!(matches!(
            pane.update(PasswordMessage::ExportBatch),
            Outcome::AskExport
        ));
        let Outcome::Write(_, text) =
            pane.update(PasswordMessage::ExportTo(Some(PathBuf::from("b.txt"))))
        else {
            panic!("written");
        };
        assert_eq!(text.lines().count(), 3);
        pane.update(PasswordMessage::Exported(Err("denied".to_owned())));
        assert_eq!(pane.export_failed.as_deref(), Some("denied"));
        assert!(export_file_name().starts_with("passwords_"));
    }

    #[test]
    fn passwords_are_never_written_out() {
        let pane = pane();
        let shown = format!("{pane:?}");
        assert!(!shown.contains(pane.engine().password()), "{shown}");
    }
}
