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

//! The log viewer, as the C# `LogViewerView` (`Views/Tools/LogViewerView.xaml.cs`): a file
//! opened by its path or picked, its newest 10,000 lines shown in the encoding chosen, a
//! filter applied on demand with its matches marked, and the file followed every 500 ms
//! while Tail is on, read again from its start when it got shorter or another file took its
//! place. Unlike the C#, a control or bidirectional character of a line is shown written
//! out, so a log cannot make its lines read otherwise than they are.

use std::collections::VecDeque;
use std::ops::Range;
use std::path::PathBuf;
use std::time::SystemTime;

use heimdall_app::TabId;
use heimdall_core::tools::date_time::Instant;
use heimdall_core::tools::log_viewer::{
    self, FileIdentity, FilterSpec, LogChunk, LogEncoding, LogFilter, LogPosition,
    MAX_DISPLAY_LINES, Severity,
};
use iced::advanced::text::highlighter::{self, Highlighter};
use iced::widget::text_editor::{Action, Content, Motion};
use iced::widget::{checkbox, column, pick_list, row, text, text_editor, text_input};
use iced::{Element, Font, Length, Task, Theme, window};

use super::{CopySlot, ToolMessage, ToolPane};
use crate::file_dialog::{self, FileDialog};
use crate::i18n::fl;
use crate::shell::{Message, main_window_task};
use crate::styles;
use crate::tokens::{font_size, spacing};

/// Bytes in a kilobyte, for the size the status line shows, as the C# `Length / 1024.0`.
const BYTES_PER_KB: f64 = 1024.0;

/// What the log viewer is asked.
#[derive(Debug, Clone)]
pub enum LogViewerMessage {
    /// The path typed.
    Path(String),
    /// Open the file at the path, as Enter in the C# path box.
    Open,
    /// Pick a file, as the C# Browse button.
    Browse,
    /// The file picked; `None` when cancelled.
    Picked(Option<PathBuf>),
    /// Read `.0` of the file brought this, or failed.
    Read(u64, Result<LogChunk, ReadError>),
    /// Tail on or off.
    Tail(bool),
    /// Time to look at the file again.
    TailTick,
    /// Follow the newest line or not.
    AutoScroll(bool),
    /// The filter typed.
    Filter(String),
    /// Apply it, as the C# Filter button and Enter.
    ApplyFilter,
    /// Clear it, as the C# Clear button.
    ClearFilter,
    /// Case sensitive or not.
    CaseSensitive(bool),
    /// The filter is a regular expression or plain text.
    Regex(bool),
    /// The encoding chosen.
    Encoding(LogEncoding),
    /// Something done in the lines shown, which does not change them.
    Lines(Action),
    /// Copy the lines shown.
    Copy,
}

/// Why a read failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// No file at the path, as `ToolLogViewErrorFileNotFound`.
    NotFound,
    /// The file could not be read: what the system said, as `ToolLogViewErrorOpen`.
    Failed(String),
}

/// What an update asks of its tab.
#[derive(Debug)]
pub enum Outcome {
    /// Nothing more.
    Done,
    /// This text copied.
    Copy(String),
    /// The open dialog.
    Browse,
    /// Read `.0` of `.1` in `.2` from `.3`.
    Read(u64, PathBuf, LogEncoding, LogPosition),
}

impl Outcome {
    /// What tab `tab` runs for it, its dialog over the window `main`.
    pub fn task(self, tab: TabId, main: Option<window::Id>) -> Task<Message> {
        match self {
            Self::Done | Self::Copy(_) => Task::none(),
            Self::Browse => pick_file(tab, main),
            Self::Read(read, path, encoding, position) => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        log_viewer::read_from(&path, encoding, &position).map_err(|error| {
                            if error.kind() == std::io::ErrorKind::NotFound {
                                ReadError::NotFound
                            } else {
                                ReadError::Failed(error.to_string())
                            }
                        })
                    })
                    .await
                    .unwrap_or_else(|error| Err(ReadError::Failed(error.to_string())))
                },
                move |result| {
                    Message::Tool(
                        tab,
                        ToolMessage::LogView(LogViewerMessage::Read(read, result)),
                    )
                },
            ),
        }
    }
}

/// What the error line says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// A read failed.
    Read(ReadError),
    /// The filter is not a regular expression, as `ToolLogViewErrorInvalidRegex`.
    InvalidRegex,
}

/// The file shown and where its next read starts.
#[derive(Debug, Clone)]
struct OpenFile {
    path: PathBuf,
    /// The encoding read with: the one chosen, or the one its byte order mark names.
    encoding: LogEncoding,
    offset: u64,
    /// The file read, to tell it from another that takes its place; `None` before a read.
    identity: Option<FileIdentity>,
    /// The last line shown has no end yet: the next read brings it again, whole.
    open_last: bool,
    len: u64,
    modified: Option<SystemTime>,
    /// A read of it came back.
    read_once: bool,
}

/// The log viewer's state, as the C# view's.
#[derive(Debug)]
pub struct LogViewerPane {
    path: String,
    encoding: LogEncoding,
    file: Option<OpenFile>,
    /// The newest lines read, at most [`MAX_DISPLAY_LINES`].
    lines: VecDeque<String>,
    /// Lines read since the file was opened, as the C# `_totalLineCount`.
    total: usize,
    /// The lines the filter keeps, shown.
    shown: usize,
    view: Content,
    /// The read running, its number; a read of a file opened before is let go.
    reading: Option<u64>,
    reads: u64,
    tail: bool,
    auto_scroll: bool,
    /// The filter typed, and the one applied with its expression.
    filter: FilterSpec,
    applied: FilterSpec,
    regex: Option<LogFilter>,
    problem: Option<Problem>,
}

impl Default for LogViewerPane {
    /// A new tab's state: nothing open, the newest line followed, as the C# `_autoScroll`.
    fn default() -> Self {
        Self {
            path: String::new(),
            encoding: LogEncoding::default(),
            file: None,
            lines: VecDeque::new(),
            total: 0,
            shown: 0,
            view: Content::new(),
            reading: None,
            reads: 0,
            tail: false,
            auto_scroll: true,
            filter: FilterSpec::default(),
            applied: FilterSpec::default(),
            regex: None,
            problem: None,
        }
    }
}

impl LogViewerPane {
    /// Applies `message`; what is then to be done.
    pub fn update(&mut self, message: LogViewerMessage) -> Outcome {
        match message {
            LogViewerMessage::Path(typed) => self.path = typed,
            LogViewerMessage::Open => return self.open(),
            LogViewerMessage::Browse => return Outcome::Browse,
            LogViewerMessage::Picked(Some(path)) => {
                self.path = path.display().to_string();
                return self.open();
            }
            LogViewerMessage::Picked(None) | LogViewerMessage::Lines(Action::Edit(_)) => {}
            LogViewerMessage::Read(read, result) => self.read(read, result),
            LogViewerMessage::Tail(on) => {
                self.tail = on;
                if on {
                    self.auto_scroll = true;
                    return self.next_read();
                }
            }
            LogViewerMessage::TailTick => return self.next_read(),
            LogViewerMessage::AutoScroll(on) => {
                self.auto_scroll = on;
                if on {
                    self.view.perform(Action::Move(Motion::DocumentEnd));
                }
            }
            LogViewerMessage::Filter(typed) => self.filter.query = typed,
            LogViewerMessage::ApplyFilter => self.apply_filter(),
            LogViewerMessage::ClearFilter => {
                self.filter.query.clear();
                self.apply_filter();
            }
            LogViewerMessage::CaseSensitive(on) => {
                self.filter.case_sensitive = on;
                self.reapply();
            }
            LogViewerMessage::Regex(on) => {
                self.filter.regex = on;
                self.reapply();
            }
            LogViewerMessage::Encoding(encoding) => {
                self.encoding = encoding;
                // The file read again in it, as the C# `OnEncodingChanged`.
                if self.file.is_some() {
                    return self.open();
                }
            }
            LogViewerMessage::Lines(action) => self.view.perform(action),
            LogViewerMessage::Copy => return Outcome::Copy(self.shown_text()),
        }
        Outcome::Done
    }

    /// Whether the file is followed: Tail on with a file open.
    #[must_use]
    pub fn tailing(&self) -> bool {
        self.tail && self.file.is_some()
    }

    /// The file at the path read from its start, what was shown let go, as the C# `LoadFile`.
    fn open(&mut self) -> Outcome {
        let path = self.path.trim();
        if path.is_empty() {
            return Outcome::Done;
        }
        let path = PathBuf::from(path);
        self.file = Some(OpenFile {
            path: path.clone(),
            encoding: self.encoding,
            offset: 0,
            identity: None,
            open_last: false,
            len: 0,
            modified: None,
            read_once: false,
        });
        self.lines.clear();
        self.total = 0;
        self.problem = None;
        self.reads += 1;
        self.reading = Some(self.reads);
        self.refresh_view();
        Outcome::Read(self.reads, path, self.encoding, LogPosition::default())
    }

    /// The next read of the file followed, unless one is running, as the C# timer's tick.
    fn next_read(&mut self) -> Outcome {
        if !self.tailing() || self.reading.is_some() {
            return Outcome::Done;
        }
        let Some(file) = &self.file else {
            return Outcome::Done;
        };
        self.reads += 1;
        self.reading = Some(self.reads);
        Outcome::Read(
            self.reads,
            file.path.clone(),
            file.encoding,
            LogPosition {
                offset: file.offset,
                identity: file.identity.clone(),
            },
        )
    }

    /// What read `read` brought: its lines added, the oldest let go past the limit.
    fn read(&mut self, read: u64, result: Result<LogChunk, ReadError>) {
        if self.reading != Some(read) {
            return;
        }
        self.reading = None;
        let chunk = match result {
            Ok(chunk) => chunk,
            // A file locked for a moment is looked at again next time, as the C# tick;
            // the first read's failure is said.
            Err(error) => {
                if self.file.as_ref().is_some_and(|file| !file.read_once) {
                    self.file = None;
                    self.problem = Some(Problem::Read(error));
                    self.refresh_view();
                }
                return;
            }
        };
        let Some(file) = &mut self.file else {
            return;
        };
        let first = !file.read_once;
        file.read_once = true;
        if chunk.restarted {
            self.lines.clear();
            self.total = 0;
        } else if file.open_last && self.lines.pop_back().is_some() {
            // The line not ended before comes again, whole.
            self.total -= 1;
        }
        let changed = chunk.restarted || !chunk.lines.is_empty() || file.open_last;
        file.encoding = chunk.encoding;
        file.offset = chunk.next_offset;
        file.identity = Some(chunk.identity);
        file.open_last = chunk.open_last;
        file.len = chunk.file_len;
        file.modified = chunk.modified;
        self.total += chunk.read;
        // Each line as it shows, its controls and bidirectional characters written out:
        // the filter, its marks and the copy all see what is shown.
        self.lines.extend(
            chunk
                .lines
                .iter()
                .map(String::as_str)
                .map(heimdall_app::visible_line),
        );
        while self.lines.len() > MAX_DISPLAY_LINES {
            self.lines.pop_front();
        }
        if changed || first {
            self.refresh_view();
        }
    }

    /// The filter typed compiled and applied, as the C# `ApplyFilter`; an invalid regular
    /// expression is said and the filter applied before kept.
    fn apply_filter(&mut self) {
        match self.filter.compile() {
            Ok(regex) => {
                self.regex = regex;
                self.applied = self.filter.clone();
                if matches!(self.problem, Some(Problem::InvalidRegex)) {
                    self.problem = None;
                }
                self.refresh_view();
            }
            Err(_) => self.problem = Some(Problem::InvalidRegex),
        }
    }

    /// The filter applied again when one is, its options changed.
    fn reapply(&mut self) {
        if !self.applied.query.trim().is_empty() {
            self.apply_filter();
        }
    }

    /// Whether the filter applied keeps `line`.
    fn keeps(&self, line: &str) -> bool {
        self.regex.as_ref().is_none_or(|regex| regex.is_match(line))
    }

    /// The lines the filter keeps, one a line.
    fn shown_text(&self) -> String {
        let kept: Vec<&str> = self
            .lines
            .iter()
            .map(String::as_str)
            .filter(|line| self.keeps(line))
            .collect();
        kept.join("\n")
    }

    /// The lines shown made again from the lines read: the newest followed, or the place
    /// read kept.
    fn refresh_view(&mut self) {
        let cursor = self.view.cursor();
        let text = self.shown_text();
        self.shown = if text.is_empty() {
            0
        } else {
            self.lines.iter().filter(|line| self.keeps(line)).count()
        };
        self.view = Content::with_text(&text);
        if self.auto_scroll {
            self.view.perform(Action::Move(Motion::DocumentEnd));
        } else {
            self.view.move_to(cursor);
        }
    }

    /// The lines read, for the tests.
    #[cfg(test)]
    fn lines(&self) -> Vec<&str> {
        self.lines.iter().map(String::as_str).collect()
    }

    /// What the status line says, as the C# `ToolLogViewStats`.
    fn stats(&self) -> Option<String> {
        let file = self.file.as_ref()?;
        let shown = self.shown;
        let total = self.total;
        #[expect(clippy::cast_precision_loss, reason = "a size in kilobytes, rounded")]
        let size = format!("{:.1}", file.len as f64 / BYTES_PER_KB);
        let time = file.modified.map(local_time).unwrap_or_default();
        Some(fl!(
            "ui-tool-logview-stats",
            shown = shown,
            total = total,
            size = size.as_str(),
            time = time.as_str()
        ))
    }

    /// What the error line says.
    fn problem_text(&self) -> Option<String> {
        Some(match self.problem.as_ref()? {
            Problem::Read(ReadError::NotFound) => fl!("ui-tool-logview-error-not-found"),
            Problem::Read(ReadError::Failed(error)) => {
                fl!("ui-tool-logview-error-open", error = error.as_str())
            }
            Problem::InvalidRegex => fl!("ui-tool-logview-error-regex"),
        })
    }

    /// The tool's page, as the C# `LogViewerView.xaml`: the path with Browse and Tail, the
    /// filter with its options, the encoding, the status, and the lines.
    pub fn view<'a>(&'a self, tab: TabId, state: &ToolPane) -> Element<'a, Message> {
        let send = move |message| Message::Tool(tab, ToolMessage::LogView(message));
        let path = row![
            text_input(&fl!("ui-tool-logview-path-placeholder"), &self.path)
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .style(styles::text_input)
                .on_input(move |typed| send(LogViewerMessage::Path(typed)))
                .on_submit(send(LogViewerMessage::Open)),
            super::action_button(
                fl!("ui-tool-logview-browse"),
                false,
                Some(send(LogViewerMessage::Browse))
            ),
            super::action_button(
                fl!("ui-tool-logview-open"),
                true,
                (!self.path.trim().is_empty()).then(|| send(LogViewerMessage::Open))
            ),
            toggle(fl!("ui-tool-logview-tail"), self.tail, move |on| send(
                LogViewerMessage::Tail(on)
            )),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let filter = row![
            text_input(
                &fl!("ui-tool-logview-filter-placeholder"),
                &self.filter.query
            )
            .size(font_size::BODY)
            .padding(super::INPUT_PADDING)
            .style(styles::text_input)
            .on_input(move |typed| send(LogViewerMessage::Filter(typed)))
            .on_submit(send(LogViewerMessage::ApplyFilter)),
            super::action_button(
                fl!("ui-tool-logview-filter"),
                false,
                Some(send(LogViewerMessage::ApplyFilter))
            ),
            super::action_button(
                fl!("ui-tool-logview-clear"),
                false,
                Some(send(LogViewerMessage::ClearFilter))
            ),
            toggle(
                fl!("ui-tool-logview-case-sensitive"),
                self.filter.case_sensitive,
                move |on| send(LogViewerMessage::CaseSensitive(on))
            ),
            toggle(fl!("ui-tool-logview-regex"), self.filter.regex, move |on| {
                send(LogViewerMessage::Regex(on))
            }),
        ]
        .spacing(spacing::SM)
        .align_y(iced::Alignment::Center);
        let encodings: Vec<EncodingChoice> =
            LogEncoding::ALL.into_iter().map(EncodingChoice).collect();
        let options = row![
            text(fl!("ui-tool-logview-encoding")).size(font_size::BODY),
            pick_list(
                encodings,
                Some(EncodingChoice(self.encoding)),
                move |choice| { send(LogViewerMessage::Encoding(choice.0)) }
            )
            .style(styles::pick_list)
            .menu_style(styles::menu)
            .text_size(font_size::BODY),
            toggle(
                fl!("ui-tool-logview-auto-scroll"),
                self.auto_scroll,
                move |on| send(LogViewerMessage::AutoScroll(on))
            ),
        ]
        .push(
            self.tailing()
                .then(|| text(fl!("ui-tool-logview-tail-active")).size(font_size::CAPTION)),
        )
        .push(iced::widget::space().width(Length::Fill))
        .push(super::copy_button(
            fl!("ui-tool-logview-copy"),
            state.copied(CopySlot::LogViewText),
            send(LogViewerMessage::Copy),
            super::COPY_PADDING,
        ))
        .spacing(spacing::MD)
        .align_y(iced::Alignment::Center);
        let lines: Element<'a, Message> = if self.shown == 0 {
            let hint = if self.file.is_some() && self.regex.is_some() && self.total > 0 {
                fl!("ui-tool-logview-no-matches")
            } else {
                fl!("ui-tool-logview-empty-state")
            };
            super::empty_state(hint)
        } else {
            text_editor(&self.view)
                .font(super::BOX_FONT)
                .size(font_size::BODY)
                .padding(super::INPUT_PADDING)
                .height(Length::Fill)
                .style(styles::text_box)
                .on_action(move |action| send(LogViewerMessage::Lines(action)))
                .highlight_with::<LogHighlighter>(self.applied.clone(), LogMark::format)
                .into()
        };
        super::tool_body(
            column![path, filter, options]
                .push(self.problem_text().map(super::error_line))
                .push(
                    self.stats()
                        .map(|stats| text(stats).size(font_size::CAPTION).style(text::secondary)),
                )
                .push(lines)
                .spacing(spacing::SM),
        )
    }
}

/// A check box of the tool's options.
fn toggle<'a>(
    label: String,
    on: bool,
    toggled: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    checkbox(on)
        .label(label)
        .on_toggle(toggled)
        .style(styles::checkbox)
        .text_size(font_size::BODY)
        .into()
}

/// `at` on this computer's clock, as the C# `LastWriteTime.ToString("yyyy-MM-dd HH:mm:ss")`.
#[must_use]
pub fn local_time(at: SystemTime) -> String {
    let seconds = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        });
    let instant = Instant::from_unix_seconds(seconds);
    let clock = instant.wall_clock(heimdall_app::time_zone::local_offset_at(instant));
    format!(
        "{:04}-{:02}-{:02} {}",
        clock.year,
        clock.month,
        clock.day,
        clock.time_text()
    )
}

/// An encoding of the list, written in the window's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EncodingChoice(LogEncoding);

impl std::fmt::Display for EncodingChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            LogEncoding::Utf8 => fl!("ui-tool-logview-encoding-utf8"),
            LogEncoding::Ascii => fl!("ui-tool-logview-encoding-ascii"),
            LogEncoding::Utf16Le => fl!("ui-tool-logview-encoding-utf16"),
            LogEncoding::Windows1252 => fl!("ui-tool-logview-encoding-windows1252"),
        })
    }
}

/// How a part of a line is marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogMark {
    /// A line naming an error: the error colour.
    Error,
    /// A line naming a warning: the warning colour.
    Warning,
    /// What the filter matched: the accent, bold, as the C# highlighted runs.
    Match,
}

impl LogMark {
    /// How it is drawn in `theme`.
    fn format(&self, theme: &Theme) -> highlighter::Format<Font> {
        let palette = theme.extended_palette();
        match self {
            Self::Error => highlighter::Format {
                color: Some(palette.danger.base.color),
                font: None,
            },
            Self::Warning => highlighter::Format {
                color: Some(palette.warning.base.color),
                font: None,
            },
            Self::Match => highlighter::Format {
                color: Some(palette.primary.base.color),
                font: Some(Font {
                    weight: iced::font::Weight::Bold,
                    ..super::BOX_FONT
                }),
            },
        }
    }
}

/// The marks of the lines shown: each line by its severity, then the filter's matches.
pub struct LogHighlighter {
    regex: Option<LogFilter>,
    line: usize,
}

impl Highlighter for LogHighlighter {
    type Settings = FilterSpec;
    type Highlight = LogMark;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, LogMark)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            regex: settings.compile().ok().flatten(),
            line: 0,
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.regex = new_settings.compile().ok().flatten();
        self.line = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.line = self.line.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.line += 1;
        let mut marks = Vec::new();
        match log_viewer::severity(line) {
            Severity::Error => marks.push((0..line.len(), LogMark::Error)),
            Severity::Warning => marks.push((0..line.len(), LogMark::Warning)),
            Severity::Normal => {}
        }
        if let Some(regex) = &self.regex {
            marks.extend(
                regex
                    .matches(line)
                    .into_iter()
                    .map(|range| (range, LogMark::Match)),
            );
        }
        marks.into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

/// The open dialog over the window `main`, as the C# `OnBrowseClick` with its log filter.
fn pick_file(tab: TabId, main: Option<window::Id>) -> Task<Message> {
    let title = fl!("ui-tool-logview-browse");
    let filter = fl!("ui-tool-logview-file-filter");
    main_window_task(main).then(move |id| {
        let dialog = FileDialog::new()
            .set_title(title.clone())
            .add_filter(filter.clone(), &["log", "txt"]);
        let pick = match id {
            Some(id) => window::run(id, move |window| dialog.set_parent(window).pick_file()),
            None => Task::done(dialog.pick_file()),
        };
        pick.then(move |pick: file_dialog::Pick<PathBuf>| {
            Task::perform(pick, move |picked| match picked {
                Ok(path) => {
                    Message::Tool(tab, ToolMessage::LogView(LogViewerMessage::Picked(path)))
                }
                Err(unavailable) => file_dialog::failed(&unavailable),
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(lines: &[&str], read: usize, next_offset: u64, open_last: bool) -> LogChunk {
        LogChunk {
            restarted: false,
            skipped: false,
            lines: lines.iter().map(|line| (*line).to_owned()).collect(),
            read,
            open_last,
            next_offset,
            encoding: LogEncoding::Utf8,
            file_len: next_offset,
            modified: None,
            identity: FileIdentity::default(),
        }
    }

    fn opened(pane: &mut LogViewerPane) -> u64 {
        pane.update(LogViewerMessage::Path("app.log".to_owned()));
        match pane.update(LogViewerMessage::Open) {
            Outcome::Read(read, _, _, LogPosition { offset: 0, .. }) => read,
            other => panic!("a read from the start, got {other:?}"),
        }
    }

    #[test]
    fn a_line_not_yet_ended_is_replaced_by_its_whole_never_doubled() {
        let mut pane = LogViewerPane::default();
        let read = opened(&mut pane);
        pane.update(LogViewerMessage::Read(
            read,
            Ok(chunk(&["one", "tw"], 2, 4, true)),
        ));
        assert_eq!(pane.lines(), ["one", "tw"]);
        let Outcome::Read(
            next,
            _,
            _,
            LogPosition {
                offset: 4,
                identity,
            },
        ) = pane.update(LogViewerMessage::Tail(true))
        else {
            panic!("the next read starts at the open line");
        };
        assert!(identity.is_some(), "of the file read");
        assert!(
            matches!(pane.update(LogViewerMessage::TailTick), Outcome::Done),
            "one read at a time"
        );
        pane.update(LogViewerMessage::Read(
            next,
            Ok(chunk(&["two", "three"], 2, 14, false)),
        ));
        assert_eq!(pane.lines(), ["one", "two", "three"]);
        assert_eq!(pane.total, 3);
    }

    #[test]
    fn a_file_that_got_shorter_starts_over_and_reads_of_another_file_are_let_go() {
        let mut pane = LogViewerPane::default();
        let first = opened(&mut pane);
        pane.update(LogViewerMessage::Read(
            first,
            Ok(chunk(&["a", "b"], 2, 4, false)),
        ));
        let second = opened(&mut pane);
        pane.update(LogViewerMessage::Read(
            first,
            Ok(chunk(&["stale"], 1, 6, false)),
        ));
        assert!(pane.lines().is_empty(), "a read of the file opened before");
        pane.update(LogViewerMessage::Read(
            second,
            Ok(chunk(&["a"], 1, 2, false)),
        ));
        let Outcome::Read(next, ..) = pane.update(LogViewerMessage::Tail(true)) else {
            panic!("a read");
        };
        let mut restarted = chunk(&["new"], 1, 4, false);
        restarted.restarted = true;
        pane.update(LogViewerMessage::Read(next, Ok(restarted)));
        assert_eq!(pane.lines(), ["new"]);
        assert_eq!(pane.total, 1);
    }

    #[test]
    fn controls_and_bidi_characters_of_a_line_are_shown_written_out() {
        let mut pane = LogViewerPane::default();
        let read = opened(&mut pane);
        pane.update(LogViewerMessage::Read(
            read,
            Ok(chunk(
                &["a\tb", "user\u{202E}gnp.exe", "x\u{1b}[2Jy\rz"],
                3,
                30,
                false,
            )),
        ));
        assert_eq!(
            pane.lines(),
            ["a\tb", r"user\u{202E}gnp.exe", r"x\u{001B}[2Jy\u{000D}z"]
        );
        assert_eq!(pane.view.line_count(), 3, "the line breaks are kept");
        assert!(!pane.shown_text().contains(['\u{202E}', '\u{1b}', '\r']));
    }

    #[test]
    fn only_the_newest_lines_are_kept() {
        let mut pane = LogViewerPane::default();
        let read = opened(&mut pane);
        let many: Vec<String> = (0..MAX_DISPLAY_LINES)
            .map(|index| index.to_string())
            .collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        pane.update(LogViewerMessage::Read(
            read,
            Ok(chunk(&many, many.len(), 10, false)),
        ));
        let Outcome::Read(next, ..) = pane.update(LogViewerMessage::Tail(true)) else {
            panic!("a read");
        };
        pane.update(LogViewerMessage::Read(
            next,
            Ok(chunk(&["last"], 1, 20, false)),
        ));
        assert_eq!(pane.lines.len(), MAX_DISPLAY_LINES);
        assert_eq!(pane.lines.front().map(String::as_str), Some("1"));
        assert_eq!(pane.lines.back().map(String::as_str), Some("last"));
        assert_eq!(pane.total, MAX_DISPLAY_LINES + 1);
    }

    #[test]
    fn the_filter_applies_on_demand_and_a_bad_regex_keeps_the_one_before() {
        let mut pane = LogViewerPane::default();
        let read = opened(&mut pane);
        pane.update(LogViewerMessage::Read(
            read,
            Ok(chunk(
                &["GET /a 200", "GET /b 500", "POST /c 200"],
                3,
                30,
                false,
            )),
        ));
        assert_eq!(pane.shown, 3);
        pane.update(LogViewerMessage::Filter("get".to_owned()));
        assert_eq!(pane.shown, 3, "typing does not filter yet");
        pane.update(LogViewerMessage::ApplyFilter);
        assert_eq!(pane.shown, 2);
        assert_eq!(pane.shown_text(), "GET /a 200\nGET /b 500");
        pane.update(LogViewerMessage::CaseSensitive(true));
        assert_eq!(pane.shown, 0);
        pane.update(LogViewerMessage::Regex(true));
        pane.update(LogViewerMessage::Filter("(".to_owned()));
        pane.update(LogViewerMessage::ApplyFilter);
        assert_eq!(pane.problem, Some(Problem::InvalidRegex));
        pane.update(LogViewerMessage::ClearFilter);
        assert_eq!(pane.problem, None);
        assert_eq!(pane.shown, 3);
    }

    #[test]
    fn a_missing_file_is_said_and_a_failed_tail_read_waits_for_the_next() {
        let mut pane = LogViewerPane::default();
        let read = opened(&mut pane);
        pane.update(LogViewerMessage::Read(read, Err(ReadError::NotFound)));
        assert_eq!(pane.problem, Some(Problem::Read(ReadError::NotFound)));
        assert!(!pane.tailing());
        let read = opened(&mut pane);
        pane.update(LogViewerMessage::Read(read, Ok(chunk(&["a"], 1, 2, false))));
        let Outcome::Read(next, ..) = pane.update(LogViewerMessage::Tail(true)) else {
            panic!("a read");
        };
        pane.update(LogViewerMessage::Read(
            next,
            Err(ReadError::Failed("locked".to_owned())),
        ));
        assert_eq!(pane.problem, None);
        assert!(matches!(
            pane.update(LogViewerMessage::TailTick),
            Outcome::Read(..)
        ));
    }

    #[test]
    fn the_highlighter_marks_severity_then_matches() {
        let mut highlighter = LogHighlighter::new(&FilterSpec {
            query: "disk".to_owned(),
            ..FilterSpec::default()
        });
        let marks: Vec<_> = highlighter.highlight_line("WARN disk low").collect();
        assert_eq!(marks, [(0..13, LogMark::Warning), (5..9, LogMark::Match)]);
        let marks: Vec<_> = highlighter.highlight_line("all fine").collect();
        assert!(marks.is_empty());
        assert_eq!(highlighter.current_line(), 2);
    }
}
