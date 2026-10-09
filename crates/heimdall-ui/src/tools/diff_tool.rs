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

//! The text comparison, as the C# `TextDiffView` and `TextDiffViewModel`: the original and
//! the modified text side by side, compared away from the window on "Compare" or Ctrl+Enter,
//! or 500 ms after the last change with "Auto-compare"; spaces or case ignored when asked.
//! The result is one list, as the C#'s: each line with its number in the original and in
//! the modified text, `-`, `+` or nothing before it, on the colour of its kind; a line
//! removed then added again compared word by word, its changed words marked. The counts sit
//! in the header with the buttons; the result copied as a unified diff.

use std::time::Duration;

use heimdall_app::TabId;
use heimdall_core::tools::diff_engine::{
    self, DEFAULT_MAX_LINE_COUNT, DiffLine, DiffLineKind, DiffLines, DiffOptions, TextDiff,
    WordSegment,
};
use iced::keyboard::{Key, key::Named};
use iced::widget::text::Span;
use iced::widget::text_editor::{Action, Binding, Content, KeyPress, Status};
use iced::widget::{Column, checkbox, column, container, rich_text, row, span, text};
use iced::{Background, Element, Font, Length, Task, Theme};

use super::{CopySlot, Marks, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// How long "Auto-compare" waits after the last change, as the C# `AutoCompareDebounceMs`.
pub const AUTO_COMPARE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Width of each line number's column, as the C#'s.
const NUMBER_WIDTH: f32 = 44.0;

/// Width of the `-` and `+` column, as the C#'s.
const PREFIX_WIDTH: f32 = 16.0;

/// Padding of a line number, as the C# `Padding="2,1,6,1"`.
const NUMBER_PADDING: iced::Padding = iced::Padding {
    top: 1.0,
    right: 6.0,
    bottom: 1.0,
    left: 2.0,
};

/// Padding of a line's prefix and text, as the C# `Padding="0,1,4,1"`.
const LINE_PADDING: iced::Padding = iced::Padding {
    top: 1.0,
    right: 4.0,
    bottom: 1.0,
    left: 0.0,
};

/// Room between the two boxes, as the C# column of 8.
const BOXES_GAP: f32 = 8.0;

/// Room between the parts of the page, as the C# `Margin="0,8"`.
const STACK_GAP: f32 = 8.0;

/// Room left of the options, as the C# `Margin="16,0,8,0"`.
const OPTIONS_LEFT: f32 = 16.0;

/// Room between the options, as the C# checkboxes' right margin.
const OPTION_GAP: f32 = 12.0;

/// Room right of the counts in the header, as the C#'s.
const STATS_GAP: f32 = 12.0;

/// What the prefix column shows for each kind, as the C#.
const UNCHANGED_PREFIX: &str = " ";
const REMOVED_PREFIX: &str = "-";
const ADDED_PREFIX: &str = "+";

/// The prefix's font: bold, as the C#'s.
const PREFIX_FONT: Font = Font {
    weight: iced::font::Weight::Bold,
    ..Font::MONOSPACE
};

/// A row of the result, as the C# `DiffLineDisplay`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    /// Its number in the original text, empty for an added line.
    pub left: String,
    /// Its number in the modified text, empty for a removed line.
    pub right: String,
    /// What became of it.
    pub kind: DiffLineKind,
    /// Its parts, changed words marked.
    pub segments: Vec<WordSegment>,
}

/// A comparison made away from the window, as the C# `DiffComputationResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Computed {
    /// What the engine gave.
    pub result: TextDiff,
    /// The rows shown.
    pub rows: Vec<DiffRow>,
    /// The line limit it was made with.
    pub max_line_count: usize,
}

/// `original` and `modified` compared with `options`, and the rows shown, as the C#
/// `ComputeDiffState` (`TextDiffViewModel.cs:250-260`).
#[must_use]
pub fn compute(original: &str, modified: &str, options: DiffOptions) -> Computed {
    let result = diff_engine::diff(original, modified, options);
    let rows = match &result {
        TextDiff::Success(lines) => rows(&lines.lines),
        TextDiff::InputTooLarge => Vec::new(),
    };
    Computed {
        result,
        rows,
        max_line_count: options.effective_max_line_count(),
    }
}

/// The rows of `lines`, as the C# `BuildDisplayItems` (`TextDiffViewModel.cs:262-331`): each
/// side's line numbers counted; a line removed just before one added, the pair compared word
/// by word.
#[must_use]
pub fn rows(lines: &[DiffLine]) -> Vec<DiffRow> {
    let mut rows = Vec::with_capacity(lines.len());
    let (mut left, mut right) = (0_usize, 0_usize);
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        match line.kind {
            DiffLineKind::Unchanged => {
                left += 1;
                right += 1;
                rows.push(DiffRow {
                    left: left.to_string(),
                    right: right.to_string(),
                    kind: DiffLineKind::Unchanged,
                    segments: plain(&line.text),
                });
                index += 1;
            }
            DiffLineKind::Removed => {
                left += 1;
                if let Some(added) = lines
                    .get(index + 1)
                    .filter(|next| next.kind == DiffLineKind::Added)
                {
                    let words = diff_engine::word_diff(&line.text, &added.text);
                    rows.push(DiffRow {
                        left: left.to_string(),
                        right: String::new(),
                        kind: DiffLineKind::Removed,
                        segments: words.old,
                    });
                    right += 1;
                    rows.push(DiffRow {
                        left: String::new(),
                        right: right.to_string(),
                        kind: DiffLineKind::Added,
                        segments: words.new,
                    });
                    index += 2;
                } else {
                    rows.push(DiffRow {
                        left: left.to_string(),
                        right: String::new(),
                        kind: DiffLineKind::Removed,
                        segments: plain(&line.text),
                    });
                    index += 1;
                }
            }
            DiffLineKind::Added => {
                right += 1;
                rows.push(DiffRow {
                    left: String::new(),
                    right: right.to_string(),
                    kind: DiffLineKind::Added,
                    segments: plain(&line.text),
                });
                index += 1;
            }
        }
    }
    rows
}

/// `text` as one part, unchanged; nothing for an empty line, as the C#
/// `CreatePlainSegments`.
fn plain(text: &str) -> Vec<WordSegment> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![WordSegment::new(text, false)]
    }
}

/// What the text comparison is asked.
#[derive(Debug, Clone)]
pub enum DiffMessage {
    /// Something done in the original's box.
    Original(Action),
    /// Something done in the modified text's box.
    Modified(Action),
    /// "Ignore whitespace" on or off.
    IgnoreWhitespace(bool),
    /// "Ignore case" on or off.
    IgnoreCase(bool),
    /// "Auto-compare" on or off.
    AutoCompare(bool),
    /// Compare, as the C# button and Ctrl+Enter.
    Compare,
    /// Swap the two texts.
    Swap,
    /// Clear both texts and the result.
    Clear,
    /// Copy the result as a unified diff.
    Copy,
    /// The wait after change `.0` is over: compared if nothing changed since.
    AutoRun(u64),
    /// A comparison made away from the window; `None` when it was lost.
    Computed(Option<Box<Computed>>),
}

/// What an update asks of its tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This text copied by the copy button.
    Copy(String),
    /// These texts compared away from the window with these options.
    Compute(String, String, DiffOptions),
    /// A comparison after [`AUTO_COMPARE_DEBOUNCE`], unless something changes before:
    /// change `.0`.
    Schedule(u64),
}

impl Outcome {
    /// What tab `tab` runs for it; a copy is its tab's.
    pub fn task(self, tab: TabId) -> Task<Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Diff(message));
        match self {
            Self::Done | Self::Copy(_) => Task::none(),
            Self::Compute(original, modified, options) => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || compute(&original, &modified, options))
                        .await
                        .ok()
                        .map(Box::new)
                },
                move |computed| send(DiffMessage::Computed(computed)),
            ),
            Self::Schedule(change) => {
                Task::perform(super::wait(AUTO_COMPARE_DEBOUNCE), move |()| {
                    send(DiffMessage::AutoRun(change))
                })
            }
        }
    }
}

/// The text comparison's state, as the C# view model's.
#[derive(Debug)]
pub struct DiffPane {
    original: Content,
    modified: Content,
    /// "Ignore whitespace" and "Ignore case".
    options: DiffOptions,
    auto_compare: bool,
    /// A comparison is being made: "Compare" waits.
    busy: bool,
    /// The last comparison was refused for its size.
    too_large: bool,
    /// The line limit of the last comparison.
    max_line_count: usize,
    /// The engine's lines of the last comparison, and their counts.
    lines: DiffLines,
    /// The rows shown.
    rows: Vec<DiffRow>,
    /// Changes made, the last one's number.
    changes: u64,
}

impl Default for DiffPane {
    fn default() -> Self {
        Self {
            original: Content::new(),
            modified: Content::new(),
            options: DiffOptions::default(),
            auto_compare: false,
            busy: false,
            too_large: false,
            max_line_count: DEFAULT_MAX_LINE_COUNT,
            lines: DiffLines::default(),
            rows: Vec::new(),
            changes: 0,
        }
    }
}

impl DiffPane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: DiffMessage) -> Outcome {
        match message {
            DiffMessage::Original(action) => {
                let edit = action.is_edit();
                self.original.perform(action);
                if edit {
                    return self.schedule();
                }
            }
            DiffMessage::Modified(action) => {
                let edit = action.is_edit();
                self.modified.perform(action);
                if edit {
                    return self.schedule();
                }
            }
            DiffMessage::IgnoreWhitespace(on) => {
                self.options.ignore_whitespace = on;
                // As the C# `OnIgnoreWhitespaceChanged`: compared again at once.
                return self.rerun();
            }
            DiffMessage::IgnoreCase(on) => {
                self.options.ignore_case = on;
                return self.rerun();
            }
            DiffMessage::AutoCompare(on) => {
                self.auto_compare = on;
                // As the C# `OnAutoCompareChanged`: off, a comparison waiting is dropped.
                if !on {
                    self.changes += 1;
                }
            }
            DiffMessage::Compare => return self.run(),
            DiffMessage::Swap => {
                // As the C# `Swap`: each text set, which schedules as typing does.
                let original = self.original_text();
                let modified = self.modified_text();
                self.original = Content::with_text(&modified);
                self.modified = Content::with_text(&original);
                return self.schedule();
            }
            DiffMessage::Clear => self.clear(),
            DiffMessage::Copy => return Outcome::Copy(self.unified_text()),
            DiffMessage::AutoRun(change) => {
                if change == self.changes {
                    return self.run();
                }
            }
            DiffMessage::Computed(computed) => {
                self.busy = false;
                if let Some(computed) = computed {
                    self.apply(*computed);
                }
            }
        }
        Outcome::Done
    }

    /// A text changed: compared once the user pauses, with "Auto-compare", as the C#
    /// `ScheduleAutoCompare`.
    fn schedule(&mut self) -> Outcome {
        self.changes += 1;
        if self.auto_compare {
            Outcome::Schedule(self.changes)
        } else {
            Outcome::Done
        }
    }

    /// An option changed: compared again at once when a text is given.
    fn rerun(&mut self) -> Outcome {
        if self.original_text().is_empty() && self.modified_text().is_empty() {
            Outcome::Done
        } else {
            self.run()
        }
    }

    /// A comparison started away from the window, as the C# `RunDiffAsync`: not while one
    /// is being made.
    fn run(&mut self) -> Outcome {
        if self.busy {
            return Outcome::Done;
        }
        self.busy = true;
        // A comparison waiting is dropped: this one answers it.
        self.changes += 1;
        Outcome::Compute(self.original_text(), self.modified_text(), self.options)
    }

    /// Both texts and the result cleared, as the C# `Clear`.
    fn clear(&mut self) {
        self.original = Content::new();
        self.modified = Content::new();
        self.changes += 1;
        self.too_large = false;
        self.max_line_count = DEFAULT_MAX_LINE_COUNT;
        self.lines = DiffLines::default();
        self.rows.clear();
    }

    /// A comparison shown, as the C# `ApplyComputationResult`
    /// (`TextDiffViewModel.cs:333-360`).
    pub fn apply(&mut self, computed: Computed) {
        self.max_line_count = computed.max_line_count;
        match computed.result {
            TextDiff::Success(lines) => {
                self.too_large = false;
                self.lines = lines;
                self.rows = computed.rows;
            }
            TextDiff::InputTooLarge => {
                self.too_large = true;
                self.lines = DiffLines::default();
                self.rows.clear();
            }
        }
    }

    /// The original text.
    #[must_use]
    pub fn original_text(&self) -> String {
        super::box_text(&self.original)
    }

    /// The modified text.
    #[must_use]
    pub fn modified_text(&self) -> String {
        super::box_text(&self.modified)
    }

    /// Whether the result is shown, rather than the empty state.
    fn has_results(&self) -> bool {
        !self.rows.is_empty()
    }

    /// The counts in the header, as the C# `RebuildStatsText`.
    #[must_use]
    pub fn stats_text(&self) -> String {
        if !self.has_results() {
            return String::new();
        }
        fl!(
            "ui-tool-diff-stats",
            added = self.lines.added,
            removed = self.lines.removed,
            unchanged = self.lines.unchanged
        )
    }

    /// The status under the result, as the C# `RebuildStatusText`.
    #[must_use]
    pub fn status_text(&self) -> String {
        if self.busy {
            return fl!("ui-tool-diff-comparing");
        }
        if self.too_large {
            return fl!("ui-tool-diff-status-too-large", max = self.max_line_count);
        }
        if self.has_results() {
            return fl!("ui-tool-diff-status-done", count = self.rows.len());
        }
        String::new()
    }

    /// The result as a unified diff, as the C# `RebuildUnifiedDiffText`: its two header
    /// lines, then each line after its `-`, `+` or space, each ended with a line break.
    #[must_use]
    pub fn unified_text(&self) -> String {
        if self.too_large || self.lines.lines.is_empty() {
            return String::new();
        }
        let mut unified = String::new();
        for header in [
            fl!("ui-tool-diff-original-header"),
            fl!("ui-tool-diff-modified-header"),
        ] {
            unified.push_str(&header);
            unified.push_str(super::NEW_LINE);
        }
        for line in &self.lines.lines {
            unified.push_str(prefix(line.kind));
            unified.push_str(&line.text);
            unified.push_str(super::NEW_LINE);
        }
        unified
    }

    /// What the header holds right of the title, as the C#'s: the counts, Compare, Swap
    /// and Clear.
    pub fn header_actions<'a>(&self, tab: TabId) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Diff(message));
        row![
            container(
                text(self.stats_text())
                    .size(font_size::BODY)
                    .style(text::secondary)
            )
            .padding(iced::Padding {
                right: STATS_GAP - spacing::XS,
                ..iced::Padding::ZERO
            }),
            super::action_button(
                fl!("ui-tool-diff-compare"),
                true,
                (!self.busy).then(|| send(DiffMessage::Compare))
            ),
            small_button(fl!("ui-tool-diff-swap"), send(DiffMessage::Swap)),
            small_button(fl!("ui-tool-diff-clear"), send(DiffMessage::Clear)),
        ]
        .spacing(spacing::XS)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// The tool's page, as the C# `TextDiffView.xaml`: the two texts side by side, the
    /// output's label with the options and the copy button, the result or the empty state,
    /// and the status.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane, marks: Marks) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Diff(message));
        let idle = !self.busy;
        let side = |label: String, content: &'a Content, placeholder: String, original: bool| {
            column![
                super::field_label(label),
                super::text_box(content, Some(placeholder))
                    .height(Length::Fill)
                    .wrapping(text::Wrapping::None)
                    .on_action(move |action| {
                        send(if original {
                            DiffMessage::Original(action)
                        } else {
                            DiffMessage::Modified(action)
                        })
                    })
                    .key_binding(move |press| binding(press, idle, send)),
            ]
            .spacing(spacing::XS)
            .width(Length::Fill)
        };
        let inputs = row![
            side(
                fl!("ui-tool-diff-original"),
                &self.original,
                fl!("ui-tool-diff-original-placeholder"),
                true
            ),
            side(
                fl!("ui-tool-diff-modified"),
                &self.modified,
                fl!("ui-tool-diff-modified-placeholder"),
                false
            ),
        ]
        .spacing(BOXES_GAP)
        .height(Length::Fill);
        let options = row![
            option(
                fl!("ui-tool-diff-ignore-whitespace"),
                self.options.ignore_whitespace,
                move |on| send(DiffMessage::IgnoreWhitespace(on))
            ),
            option(
                fl!("ui-tool-diff-ignore-case"),
                self.options.ignore_case,
                move |on| send(DiffMessage::IgnoreCase(on))
            ),
            option(
                fl!("ui-tool-diff-auto-compare"),
                self.auto_compare,
                move |on| send(DiffMessage::AutoCompare(on))
            ),
        ]
        .spacing(OPTION_GAP);
        let toolbar = row![
            super::field_label(fl!("ui-tool-diff-output")),
            container(options)
                .padding(iced::Padding {
                    left: OPTIONS_LEFT,
                    right: spacing::SM,
                    ..iced::Padding::ZERO
                })
                .width(Length::Fill),
            super::copy_button(
                fl!("ui-tool-diff-copy"),
                state.copied(CopySlot::DiffUnified),
                send(DiffMessage::Copy),
                super::COPY_PADDING,
            ),
        ]
        .align_y(iced::Alignment::Center);
        let result: Element<'a, Message> = if self.has_results() {
            // Scrolled down only: a line wider than the list is cut at its edge, as iced
            // cannot both fill a row's colour across and scroll it sideways.
            let list = Column::with_children(self.rows.iter().map(|shown| diff_row(shown, marks)))
                .width(Length::Fill);
            container(styles::scroll(list).height(Length::Fill))
                .height(Length::Fill)
                .style(styles::field_box)
                .into()
        } else {
            super::empty_state(fl!("ui-tool-diff-empty"))
        };
        super::tool_body(
            column![
                inputs,
                toolbar,
                result,
                text(self.status_text())
                    .size(font_size::CAPTION)
                    .style(text::secondary),
            ]
            .spacing(STACK_GAP),
        )
    }
}

/// What the unified diff writes before a line of `kind`.
fn prefix(kind: DiffLineKind) -> &'static str {
    match kind {
        DiffLineKind::Unchanged => UNCHANGED_PREFIX,
        DiffLineKind::Removed => REMOVED_PREFIX,
        DiffLineKind::Added => ADDED_PREFIX,
    }
}

/// A row of the result, as the C# item template: the two numbers, the prefix and the
/// line, on the colour of its kind, its changed words on a stronger one.
fn diff_row<'a>(shown: &'a DiffRow, marks: Marks) -> Element<'a, Message> {
    let kind = shown.kind;
    let number = |value: &'a str| {
        container(
            text(value)
                .font(super::BOX_FONT)
                .size(font_size::BODY)
                .style(text::secondary)
                .align_x(iced::alignment::Horizontal::Right)
                .width(Length::Fill),
        )
        .padding(NUMBER_PADDING)
        .width(NUMBER_WIDTH)
    };
    let prefix_color = match kind {
        DiffLineKind::Unchanged => marks.secondary,
        DiffLineKind::Removed => marks.removed,
        DiffLineKind::Added => marks.added,
    };
    let word = match kind {
        DiffLineKind::Unchanged => None,
        DiffLineKind::Removed => Some(marks.removed_word),
        DiffLineKind::Added => Some(marks.added_word),
    };
    let spans: Vec<Span<'a, (), Font>> = shown
        .segments
        .iter()
        .map(|segment| {
            let part = span(segment.text.as_str()).color(marks.text);
            match word {
                Some(color) if segment.changed => part.background(color),
                _ => part,
            }
        })
        .collect();
    let line = row![
        number(&shown.left),
        number(&shown.right),
        container(
            text(prefix(kind))
                .font(PREFIX_FONT)
                .size(font_size::BODY_LARGE)
                .color(prefix_color)
        )
        .padding(LINE_PADDING)
        .width(PREFIX_WIDTH),
        container(
            rich_text(spans)
                .font(super::BOX_FONT)
                .size(font_size::BODY_LARGE)
                .wrapping(text::Wrapping::None)
        )
        .padding(LINE_PADDING),
    ]
    .align_y(iced::Alignment::Center);
    let background = match kind {
        DiffLineKind::Unchanged => None,
        DiffLineKind::Removed => Some(marks.removed_line),
        DiffLineKind::Added => Some(marks.added_line),
    };
    container(line)
        .width(Length::Fill)
        .clip(true)
        .style(move |_: &Theme| container::Style {
            background: background.map(Background::Color),
            ..container::Style::default()
        })
        .into()
}

/// A secondary button of the header, as the C#'s with `PaddingButtonCopy`.
fn small_button<'a>(label: String, on_press: Message) -> Element<'a, Message> {
    iced::widget::button(text(label).size(font_size::BODY))
        .padding(super::COPY_PADDING)
        .style(styles::secondary)
        .on_press(on_press)
        .into()
}

/// An option's checkbox.
fn option<'a>(
    label: String,
    on: bool,
    toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    checkbox(on)
        .label(label)
        .on_toggle(toggle)
        .style(styles::checkbox)
        .text_size(font_size::BODY)
        .into()
}

/// A text box's keys, as the C# `OnDiffInputPreviewKeyDown`: Ctrl+Enter compares, while
/// `idle`; the rest as any text box.
fn binding(
    press: KeyPress,
    idle: bool,
    send: impl Fn(DiffMessage) -> Message,
) -> Option<Binding<Message>> {
    if idle
        && matches!(press.status, Status::Focused { .. })
        && press.key == Key::Named(Named::Enter)
        && press.modifiers.control()
        && !press.modifiers.shift()
        && !press.modifiers.alt()
    {
        return Some(Binding::Custom(send(DiffMessage::Compare)));
    }
    Binding::from_key_press(press)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    fn paste(text: &str) -> Action {
        Action::Edit(Edit::Paste(Arc::new(text.to_owned())))
    }

    /// `pane` compared as its tab would, the work done at once.
    fn compared(pane: &mut DiffPane) {
        let Outcome::Compute(original, modified, options) = pane.update(DiffMessage::Compare)
        else {
            panic!("compared");
        };
        assert!(pane.busy);
        assert_eq!(pane.status_text(), "Comparing...");
        let computed = compute(&original, &modified, options);
        let _ = pane.update(DiffMessage::Computed(Some(Box::new(computed))));
    }

    fn filled(original: &str, modified: &str) -> DiffPane {
        let mut pane = DiffPane::default();
        let _ = pane.update(DiffMessage::Original(paste(original)));
        let _ = pane.update(DiffMessage::Modified(paste(modified)));
        pane
    }

    #[test]
    fn a_comparison_shows_its_rows_counts_and_status() {
        let mut pane = filled("a\nb\nc", "a\nx\nc");
        compared(&mut pane);
        assert!(!pane.busy);
        assert_eq!(pane.rows.len(), 4);
        assert_eq!(pane.stats_text(), "+1 additions, -1 deletions, 2 unchanged");
        assert_eq!(pane.status_text(), "Diff complete: 4 lines");
        let numbers: Vec<(&str, &str)> = pane
            .rows
            .iter()
            .map(|shown| (shown.left.as_str(), shown.right.as_str()))
            .collect();
        assert_eq!(numbers, [("1", "1"), ("2", ""), ("", "2"), ("3", "3")]);
        let unified = pane.unified_text();
        let nl = super::super::NEW_LINE;
        assert_eq!(
            unified,
            format!("--- original{nl}+++ modified{nl} a{nl}-b{nl}+x{nl} c{nl}")
        );
    }

    #[test]
    fn only_a_removed_line_followed_by_an_added_one_is_compared_word_by_word() {
        let lines = [
            DiffLine {
                kind: DiffLineKind::Removed,
                text: "one two".to_owned(),
            },
            DiffLine {
                kind: DiffLineKind::Removed,
                text: "three".to_owned(),
            },
            DiffLine {
                kind: DiffLineKind::Added,
                text: "three four".to_owned(),
            },
            DiffLine {
                kind: DiffLineKind::Added,
                text: "five".to_owned(),
            },
        ];
        let shown = rows(&lines);
        assert_eq!(shown[0].segments, [WordSegment::new("one two", false)]);
        assert_eq!(
            shown[2].segments,
            [
                WordSegment::new("three", false),
                WordSegment::new(" four", true)
            ]
        );
        assert_eq!(shown[3].segments, [WordSegment::new("five", false)]);
    }

    #[test]
    fn auto_compare_runs_after_a_pause_and_options_rerun_at_once() {
        let mut pane = DiffPane::default();
        assert_eq!(
            pane.update(DiffMessage::Original(paste("a"))),
            Outcome::Done,
            "auto-compare is off"
        );
        let _ = pane.update(DiffMessage::AutoCompare(true));
        let Outcome::Schedule(first) = pane.update(DiffMessage::Modified(paste("b"))) else {
            panic!("scheduled");
        };
        let Outcome::Schedule(second) = pane.update(DiffMessage::Modified(paste("c"))) else {
            panic!("scheduled");
        };
        assert_eq!(pane.update(DiffMessage::AutoRun(first)), Outcome::Done);
        assert!(matches!(
            pane.update(DiffMessage::AutoRun(second)),
            Outcome::Compute(..)
        ));
        let _ = pane.update(DiffMessage::Computed(None));
        assert!(matches!(
            pane.update(DiffMessage::IgnoreCase(true)),
            Outcome::Compute(
                _,
                _,
                DiffOptions {
                    ignore_case: true,
                    ..
                }
            )
        ));
        assert_eq!(
            pane.update(DiffMessage::Compare),
            Outcome::Done,
            "not while one is being made"
        );
        let mut empty = DiffPane::default();
        assert_eq!(
            empty.update(DiffMessage::IgnoreWhitespace(true)),
            Outcome::Done
        );
    }

    #[test]
    fn swap_clear_and_a_text_too_large() {
        let mut pane = filled("left", "right");
        let _ = pane.update(DiffMessage::Swap);
        assert_eq!(
            (pane.original_text(), pane.modified_text()),
            ("right".to_owned(), "left".to_owned())
        );
        compared(&mut pane);
        let _ = pane.update(DiffMessage::Clear);
        assert!(pane.original_text().is_empty() && pane.modified_text().is_empty());
        assert!(pane.rows.is_empty());
        assert!(pane.stats_text().is_empty() && pane.status_text().is_empty());
        assert!(pane.unified_text().is_empty());
        let over = vec!["x"; DEFAULT_MAX_LINE_COUNT + 1].join("\n");
        let mut pane = filled(&over, "");
        compared(&mut pane);
        assert!(pane.rows.is_empty() && pane.stats_text().is_empty());
        assert_eq!(
            pane.status_text(),
            format!("Input exceeds {DEFAULT_MAX_LINE_COUNT} lines. Please reduce the text size.")
        );
    }
}
