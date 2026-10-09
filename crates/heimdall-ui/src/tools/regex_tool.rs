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

//! The regular expression tester, as the C# `RegexTesterView` and `RegexTesterViewModel`:
//! the pattern tried on the test text 300 ms after the last change, with the options
//! "Ignore case", "Multiline" and "Singleline"; the matches highlighted over the test text,
//! counted and listed with their groups, the first 500 of them; the list copied.
//!
//! The engine is the core's ([`regex_engine`]): what .NET has and it has not is refused
//! with its name, on the status line.

use std::time::Duration;

use heimdall_app::TabId;
use heimdall_core::tools::regex_engine::{
    self, DotnetConstruct, InvalidPattern, RegexMatch, RegexOptions, RegexTest,
};
use iced::widget::text::Span;
use iced::widget::text_editor::{Action, Content};
use iced::widget::{
    Column, checkbox, column, container, rich_text, row, span, stack, text, text_input,
};
use iced::{Element, Font, Length, Task, font};

use super::{CopySlot, Marks, ToolMessage, ToolPane};
use crate::i18n::fl;
use crate::shell::Message;
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Most matches listed, as the C# `MaxDisplayedMatches`.
pub const MAX_DISPLAYED_MATCHES: usize = 500;

/// How long the tester waits after a change before trying the pattern, as the C#
/// `DebounceDelay`.
pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// Room between the parts of the page, as the C# `MarginStackItem`.
const STACK_GAP: f32 = 8.0;

/// Room between the options, as the C# checkboxes' right margin.
const OPTION_GAP: f32 = 16.0;

/// The truncation notice's font: italic, as the C#'s.
const NOTICE_FONT: Font = Font {
    style: font::Style::Italic,
    ..Font::DEFAULT
};

/// What a part of the test text is, as the C# `RegexHighlightKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    /// Not matched.
    Normal,
    /// A match.
    Match,
    /// A match a named group took part in.
    NamedGroupMatch,
}

/// What the status line says, as the C# `RegexStatusKind`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RegexStatus {
    /// Nothing.
    #[default]
    None,
    /// The pattern is valid.
    Valid,
    /// The pattern was refused.
    Invalid(InvalidPattern),
}

impl RegexStatus {
    /// What it says, and whether it is an error, drawn in the error colour.
    fn said(&self) -> Option<(String, bool)> {
        let said = match self {
            Self::None => return None,
            Self::Valid => (fl!("ui-tool-regex-status-valid"), false),
            Self::Invalid(InvalidPattern::Syntax(error)) => (
                fl!("ui-tool-regex-status-invalid", error = error.as_str()),
                true,
            ),
            Self::Invalid(InvalidPattern::Unsupported(construct)) => {
                (unsupported_label(*construct), true)
            }
        };
        Some(said)
    }
}

/// What the status line says of a .NET construct the engine lacks.
fn unsupported_label(construct: DotnetConstruct) -> String {
    match construct {
        DotnetConstruct::LookAround => fl!("ui-tool-regex-unsupported-lookaround"),
        DotnetConstruct::Backreference => fl!("ui-tool-regex-unsupported-backreference"),
        DotnetConstruct::AtomicGroup => fl!("ui-tool-regex-unsupported-atomic"),
        DotnetConstruct::Conditional => fl!("ui-tool-regex-unsupported-conditional"),
    }
}

/// What the regular expression tester is asked.
#[derive(Debug, Clone)]
pub enum RegexMessage {
    /// The pattern typed.
    Pattern(String),
    /// Something done in the test text's box.
    Test(Action),
    /// "Ignore case" on or off.
    IgnoreCase(bool),
    /// "Multiline" on or off.
    Multiline(bool),
    /// "Singleline" on or off.
    Singleline(bool),
    /// The wait after change `.0` is over: the pattern tried if nothing changed since.
    Run(u64),
    /// Copy the matches listed.
    Copy,
}

/// What an update asks of its tab.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This text copied by the copy button.
    Copy(String),
    /// The pattern tried after [`DEBOUNCE`], unless something changes before: change `.0`.
    Schedule(u64),
}

impl Outcome {
    /// What tab `tab` runs for it; a copy is its tab's.
    pub fn task(self, tab: TabId) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(_) => Task::none(),
            Self::Schedule(change) => Task::perform(super::wait(DEBOUNCE), move |()| {
                Message::Tool(tab, ToolMessage::Regex(RegexMessage::Run(change)))
            }),
        }
    }
}

/// The regular expression tester's state, as the C# view model's.
#[derive(Debug)]
pub struct RegexPane {
    pattern: String,
    test: Content,
    options: RegexOptions,
    status: RegexStatus,
    /// The empty state is shown.
    empty_state: bool,
    /// The matches' panel is shown.
    results: bool,
    /// How many matches the last run found.
    total: usize,
    /// The matches listed, at most [`MAX_DISPLAYED_MATCHES`].
    matches: Vec<RegexMatch>,
    /// The test text cut into parts matched or not, drawn over its box.
    highlights: Vec<(String, HighlightKind)>,
    /// Changes made, the last one's number.
    changes: u64,
}

impl Default for RegexPane {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            test: Content::new(),
            options: RegexOptions::default(),
            status: RegexStatus::None,
            empty_state: true,
            results: false,
            total: 0,
            matches: Vec::new(),
            highlights: Vec::new(),
            changes: 0,
        }
    }
}

impl RegexPane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: RegexMessage) -> Outcome {
        match message {
            RegexMessage::Pattern(pattern) => {
                self.pattern = pattern;
                return self.schedule();
            }
            RegexMessage::Test(action) => {
                let edit = action.is_edit();
                self.test.perform(action);
                if edit {
                    return self.schedule();
                }
            }
            RegexMessage::IgnoreCase(on) => {
                self.options.ignore_case = on;
                return self.schedule();
            }
            RegexMessage::Multiline(on) => {
                self.options.multiline = on;
                return self.schedule();
            }
            RegexMessage::Singleline(on) => {
                self.options.singleline = on;
                return self.schedule();
            }
            RegexMessage::Run(change) => {
                if change == self.changes {
                    self.execute();
                }
            }
            RegexMessage::Copy => return Outcome::Copy(self.copy_text()),
        }
        Outcome::Done
    }

    /// A change made: the pattern tried once the user pauses, as the C# `ScheduleMatch`.
    fn schedule(&mut self) -> Outcome {
        self.changes += 1;
        Outcome::Schedule(self.changes)
    }

    /// The test text.
    #[must_use]
    pub fn test_text(&self) -> String {
        super::box_text(&self.test)
    }

    /// The pattern tried on the test text, as the C# `ExecuteMatch`
    /// (`RegexTesterViewModel.cs:161-205`).
    fn execute(&mut self) {
        self.total = 0;
        self.matches.clear();
        self.highlights.clear();
        if self.pattern.is_empty() {
            self.status = RegexStatus::None;
            self.empty_state = true;
            self.results = false;
            return;
        }
        let test = self.test_text();
        match regex_engine::test(&self.pattern, &test, self.options) {
            RegexTest::EmptyPattern => {
                self.status = RegexStatus::None;
                self.empty_state = true;
                self.results = false;
            }
            RegexTest::InvalidPattern(refusal) => {
                self.status = RegexStatus::Invalid(refusal);
                self.empty_state = false;
                self.results = false;
            }
            RegexTest::Success(mut matches) => {
                self.status = RegexStatus::Valid;
                self.empty_state = false;
                if test.is_empty() {
                    self.results = false;
                    return;
                }
                self.results = true;
                self.total = matches.len();
                self.highlights = highlights(&test, &matches);
                matches.truncate(MAX_DISPLAYED_MATCHES);
                self.matches = matches;
            }
        }
    }

    /// The line of match `index`, with its groups, as the C# `MatchDisplayItem`.
    fn match_line(index: usize, found: &RegexMatch) -> String {
        let mut line = fl!(
            "ui-tool-regex-match-entry",
            number = index,
            index = found.index,
            value = found.value.as_str()
        );
        for group in &found.groups {
            line.push_str(&fl!(
                "ui-tool-regex-group-entry",
                number = group.number,
                value = group.value.as_str()
            ));
        }
        line
    }

    /// The notice that the list stops at [`MAX_DISPLAYED_MATCHES`], when it does.
    fn truncated_notice(&self) -> Option<String> {
        (self.total > MAX_DISPLAYED_MATCHES).then(|| {
            fl!(
                "ui-tool-regex-truncated",
                shown = MAX_DISPLAYED_MATCHES,
                total = self.total
            )
        })
    }

    /// What the copy button copies, as the C# `UpdateMatchesCopyText`: each line listed and
    /// the truncation notice, each ended with a line break.
    #[must_use]
    pub fn copy_text(&self) -> String {
        let mut copied = String::new();
        let lines = self
            .matches
            .iter()
            .enumerate()
            .map(|(index, found)| Self::match_line(index, found))
            .chain(self.truncated_notice());
        for line in lines {
            copied.push_str(&line);
            copied.push_str(super::NEW_LINE);
        }
        copied
    }

    /// The tool's page, as the C# `RegexTesterView.xaml`: the pattern, the options, the test
    /// text with the matches over it, the count and the status, the matches listed or the
    /// empty state, and the copy button.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane, marks: Marks) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::Regex(message));
        let pattern = text_input(&fl!("ui-tool-regex-pattern-placeholder"), &self.pattern)
            .font(super::BOX_FONT)
            .size(font_size::BODY_LARGE)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |typed| send(RegexMessage::Pattern(typed)));
        let options = row![
            option(
                fl!("ui-tool-regex-ignore-case"),
                self.options.ignore_case,
                move |on| send(RegexMessage::IgnoreCase(on))
            ),
            option(
                fl!("ui-tool-regex-multiline"),
                self.options.multiline,
                move |on| send(RegexMessage::Multiline(on))
            ),
            option(
                fl!("ui-tool-regex-singleline"),
                self.options.singleline,
                move |on| send(RegexMessage::Singleline(on))
            ),
        ]
        .spacing(OPTION_GAP);
        let test_box = super::text_box(&self.test, Some(fl!("ui-tool-regex-test-placeholder")))
            .height(Length::Fill)
            .on_action(move |action| send(RegexMessage::Test(action)));
        // The matches drawn over the test text, as the C# `HighlightDisplay`: it lets the
        // pointer through to the box under it.
        let test = stack![test_box].push((!self.highlights.is_empty()).then(|| {
            container(
                rich_text(
                    self.highlights
                        .iter()
                        .map(|(part, kind)| highlight_span(part, *kind, marks))
                        .collect::<Vec<Span<'a, (), Font>>>(),
                )
                .font(super::BOX_FONT)
                .size(font_size::BODY),
            )
            .padding(super::INPUT_PADDING)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .style(styles::field_box)
        }));
        let count = if self.results {
            fl!("ui-tool-regex-count", count = self.total)
        } else {
            String::new()
        };
        let counts = row![
            text(count)
                .size(font_size::BODY)
                .style(text::secondary)
                .width(Length::Fill),
            super::status_line(self.status.said()),
        ]
        .align_y(iced::Alignment::Center);
        let below: Element<'a, Message> = if self.empty_state {
            super::empty_state(fl!("ui-tool-regex-empty"))
        } else if self.results {
            self.matches_panel()
        } else {
            container(column![]).height(Length::Fill).into()
        };
        let footer = row![
            iced::widget::space::horizontal(),
            super::copy_button(
                fl!("ui-tool-regex-copy"),
                state.copied(CopySlot::RegexMatches),
                send(RegexMessage::Copy),
                super::COPY_PADDING,
            ),
        ];
        super::tool_body(
            column![
                super::field_label(fl!("ui-tool-regex-pattern")),
                pattern,
                options,
                super::field_label(fl!("ui-tool-regex-test-text")),
                test,
                counts,
                below,
                footer,
            ]
            .spacing(STACK_GAP),
        )
    }

    /// The matches listed, as the C# `MatchesPanel`: its label, the list, and the notice
    /// that it stops at [`MAX_DISPLAYED_MATCHES`].
    fn matches_panel<'a>(&self) -> Element<'a, Message> {
        let list = Column::with_children(self.matches.iter().enumerate().map(|(index, found)| {
            text(Self::match_line(index, found))
                .font(super::BOX_FONT)
                .size(font_size::BODY)
                .into()
        }))
        .spacing(spacing::XS)
        .padding(super::INPUT_PADDING)
        .width(Length::Fill);
        column![
            super::field_label(fl!("ui-tool-regex-matches")),
            container(styles::scroll(list).height(Length::Fill))
                .height(Length::Fill)
                .style(styles::field_box),
        ]
        .push(self.truncated_notice().map(|notice| {
            text(notice)
                .size(font_size::BODY)
                .font(NOTICE_FONT)
                .style(text::secondary)
        }))
        .spacing(spacing::XS)
        .height(Length::Fill)
        .into()
    }
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

/// A part of the test text, on the colour of its kind, as the C#'s runs.
fn highlight_span(part: &str, kind: HighlightKind, marks: Marks) -> Span<'_, (), Font> {
    let shown = span(part).color(marks.text);
    match kind {
        HighlightKind::Normal => shown,
        HighlightKind::Match => shown.background(marks.regex_match),
        HighlightKind::NamedGroupMatch => shown.background(marks.regex_group),
    }
}

/// `input` cut into parts matched or not, as the C# `BuildHighlightSegments`
/// (`RegexTesterViewModel.cs:236-265`): a match a named group took part in marked so.
#[must_use]
pub fn highlights(input: &str, matches: &[RegexMatch]) -> Vec<(String, HighlightKind)> {
    if matches.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut last_end = 0;
    for found in matches {
        if found.bytes.start > last_end {
            parts.push((
                input[last_end..found.bytes.start].to_owned(),
                HighlightKind::Normal,
            ));
        }
        let kind = if found
            .groups
            .iter()
            .any(|group| group.named && group.length > 0)
        {
            HighlightKind::NamedGroupMatch
        } else {
            HighlightKind::Match
        };
        parts.push((found.value.clone(), kind));
        last_end = found.bytes.end;
    }
    if last_end < input.len() {
        parts.push((input[last_end..].to_owned(), HighlightKind::Normal));
    }
    parts
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::widget::text_editor::Edit;

    use super::*;

    fn tested(pattern: &str, test: &str) -> RegexPane {
        let mut pane = RegexPane::default();
        let _ = pane.update(RegexMessage::Pattern(pattern.to_owned()));
        let _ = pane.update(RegexMessage::Test(Action::Edit(Edit::Paste(Arc::new(
            test.to_owned(),
        )))));
        pane.execute();
        pane
    }

    #[test]
    fn an_empty_pattern_shows_the_empty_state() {
        let pane = tested("", "abc");
        assert!(pane.empty_state && !pane.results);
        assert_eq!(&pane.status, &RegexStatus::None);
    }

    #[test]
    fn a_valid_pattern_without_test_text_says_so_without_results() {
        let pane = tested("a", "");
        assert_eq!(&pane.status, &RegexStatus::Valid);
        assert!(!pane.empty_state && !pane.results);
        assert_eq!(pane.status.said(), Some(("Valid regex".to_owned(), false)));
    }

    #[test]
    fn an_invalid_pattern_after_a_success_clears_the_matches() {
        let mut pane = tested("b", "abc");
        assert_eq!(pane.matches.len(), 1);
        let _ = pane.update(RegexMessage::Pattern("(".to_owned()));
        pane.execute();
        assert!(pane.matches.is_empty() && pane.highlights.is_empty());
        assert!(!pane.results);
        let (said, error) = pane.status.said().expect("said");
        assert!(error && said.starts_with("Invalid regex: "), "{said}");
        let _ = pane.update(RegexMessage::Pattern("a(?=b)".to_owned()));
        pane.execute();
        assert_eq!(
            &pane.status,
            &RegexStatus::Invalid(InvalidPattern::Unsupported(DotnetConstruct::LookAround))
        );
    }

    #[test]
    fn no_match_shows_the_results_and_a_zero_count() {
        let pane = tested("z", "abc");
        assert!(pane.results);
        assert_eq!(fl!("ui-tool-regex-count", count = pane.total), "0 matches");
        assert!(pane.highlights.is_empty());
    }

    #[test]
    fn matches_are_listed_highlighted_and_copied() {
        let pane = tested("(c)", "abcd");
        assert_eq!(pane.matches.len(), 1);
        let line = RegexPane::match_line(0, &pane.matches[0]);
        assert!(line.starts_with("[0] Index 2"), "{line}");
        assert!(pane.copy_text().contains("Group 1"), "{}", pane.copy_text());
        assert_eq!(
            pane.highlights,
            [
                ("ab".to_owned(), HighlightKind::Normal),
                ("c".to_owned(), HighlightKind::Match),
                ("d".to_owned(), HighlightKind::Normal)
            ]
        );
        let named = tested("(?<x>c)", "abcd");
        assert_eq!(named.highlights[1].1, HighlightKind::NamedGroupMatch);
    }

    #[test]
    fn the_list_stops_at_five_hundred_and_says_so_in_the_copy() {
        let pane = tested("a", &"a".repeat(MAX_DISPLAYED_MATCHES + 1));
        assert_eq!(pane.matches.len(), MAX_DISPLAYED_MATCHES);
        assert_eq!(pane.total, MAX_DISPLAYED_MATCHES + 1);
        assert!(
            pane.copy_text()
                .contains("Showing first 500 of 501 matches"),
            "{}",
            pane.copy_text()
        );
    }

    #[test]
    fn a_change_runs_once_the_user_pauses_and_options_reach_the_engine() {
        let mut pane = RegexPane::default();
        let Outcome::Schedule(first) = pane.update(RegexMessage::Pattern("ABC".to_owned())) else {
            panic!("scheduled");
        };
        let Outcome::Schedule(second) = pane.update(RegexMessage::Test(Action::Edit(Edit::Paste(
            Arc::new("abc".to_owned()),
        )))) else {
            panic!("scheduled");
        };
        let _ = pane.update(RegexMessage::Run(first));
        assert_eq!(&pane.status, &RegexStatus::None, "a later change waits");
        let _ = pane.update(RegexMessage::Run(second));
        assert!(pane.matches.is_empty());
        let Outcome::Schedule(third) = pane.update(RegexMessage::IgnoreCase(true)) else {
            panic!("scheduled");
        };
        let _ = pane.update(RegexMessage::Run(third));
        assert_eq!(pane.matches.len(), 1);
    }
}
