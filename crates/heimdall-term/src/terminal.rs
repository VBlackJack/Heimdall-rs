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

//! The terminal: bytes in, a screen to draw and replies to send out.
//!
//! Owned by one thread, the UI's. Every event alacritty raises is drained after each
//! [`Terminal::feed`], in arrival order, so replies keep the order of the queries that caused
//! them.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, Osc52, Term, point_to_viewport, viewport_to_point};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, StdSyncHandler};

use crate::mode::InputMode;
use crate::palette::{Palette, Rgb, dim};
use crate::working_directory::WorkingDirectoryScanner;

/// Lines of history kept above the screen.
pub const DEFAULT_SCROLLBACK_LINES: usize = 10_000;

/// Smallest grid alacritty accepts: its last column is `columns - 1`.
pub const MIN_COLUMNS: usize = 2;
/// Smallest number of visible lines.
pub const MIN_ROWS: usize = 1;

/// Slot of the cursor colour among the emulator's colours.
const CURSOR_COLOR_SLOT: usize = NamedColor::Cursor as usize;

/// Size of the grid in character cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    /// Columns.
    pub cols: usize,
    /// Visible rows.
    pub rows: usize,
}

impl GridSize {
    /// This size raised to the smallest grid the emulator accepts.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            cols: self.cols.max(MIN_COLUMNS),
            rows: self.rows.max(MIN_ROWS),
        }
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// Size of one character cell in pixels, for applications that ask for the window size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPixels {
    /// Width.
    pub width: u16,
    /// Height.
    pub height: u16,
}

/// Whether the server may write to the local clipboard (OSC 52). Reading it is never
/// allowed, whatever this says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardPolicy {
    /// OSC 52 ignored.
    Disabled,
    /// The server may set the clipboard, for example copying from tmux or vim over SSH.
    AllowWrite,
}

impl ClipboardPolicy {
    /// The emulator setting. First of two barriers against the server reading the
    /// clipboard: no policy maps to a setting that emits read requests, and
    /// `drain_events` never answers one anyway.
    fn osc52(self) -> Osc52 {
        match self {
            Self::Disabled => Osc52::Disabled,
            Self::AllowWrite => Osc52::OnlyCopy,
        }
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::term::Osc52;

    use super::ClipboardPolicy;

    #[test]
    fn no_policy_lets_the_emulator_raise_clipboard_reads() {
        // The end-to-end test cannot see this barrier: the second one hides it.
        for policy in [ClipboardPolicy::Disabled, ClipboardPolicy::AllowWrite] {
            assert!(
                !matches!(policy.osc52(), Osc52::OnlyPaste | Osc52::CopyPaste),
                "{policy:?}"
            );
        }
    }
}

/// Settings of a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalConfig {
    /// Lines of history.
    pub scrollback_lines: usize,
    /// OSC 52 policy.
    pub clipboard: ClipboardPolicy,
    /// Colours.
    pub palette: Palette,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
            clipboard: ClipboardPolicy::Disabled,
            palette: Palette::default(),
        }
    }
}

/// A title change requested by the server. The text is untrusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleChange {
    /// New title.
    Set(String),
    /// Back to the default title.
    Reset,
}

/// What processing output produced besides the screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedOutput {
    /// Bytes to write back to the server: answers to its queries, in query order.
    pub replies: Vec<u8>,
    /// Last title change, if any.
    pub title: Option<TitleChange>,
    /// Whether the bell rang.
    pub bell: bool,
    /// Text the server asked to put on the clipboard, when the policy allows it.
    pub clipboard: Option<String>,
    /// Whether the screen may have changed.
    pub redraw: bool,
    /// The last working folder the server reported (OSC 7), an absolute path decoded, the
    /// host left aside, or an absolute Windows path (`ConEmu`'s OSC 9;9), as the module
    /// `working_directory` says: untrusted. Read as the output arrives, so only [`Terminal::feed`]
    /// gives one, a synchronized update or not.
    pub working_directory: Option<String>,
}

/// How a cell's text is underlined.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Underline {
    /// Not underlined.
    #[default]
    None,
    /// Single line.
    Single,
    /// Double line.
    Double,
    /// Wavy line.
    Curl,
    /// Dotted line.
    Dotted,
    /// Dashed line.
    Dashed,
}

/// How wide a cell is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CellWidth {
    /// One column.
    #[default]
    Single,
    /// The first of the two columns of a wide character.
    Wide,
    /// The second column of a wide character, or padding before one that did not fit at the
    /// end of a line: not drawn.
    Spacer,
}

/// One cell ready to draw: colours resolved, inverse, dim and hidden applied.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent text attributes, each on or off"
)]
pub struct ScreenCell {
    /// Character; a space for spacers and tabs.
    pub ch: char,
    /// Combining characters drawn over `ch`.
    pub zerowidth: Vec<char>,
    /// Text colour.
    pub fg: Rgb,
    /// Background colour.
    pub bg: Rgb,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline style.
    pub underline: Underline,
    /// Underline colour when it differs from the text colour.
    pub underline_color: Option<Rgb>,
    /// Struck through.
    pub strikeout: bool,
    /// Width.
    pub width: CellWidth,
    /// Inside the selection.
    pub selected: bool,
}

impl ScreenCell {
    fn blank(palette: &Palette) -> Self {
        Self {
            ch: ' ',
            zerowidth: Vec::new(),
            fg: palette.foreground,
            bg: palette.background,
            bold: false,
            italic: false,
            underline: Underline::None,
            underline_color: None,
            strikeout: false,
            width: CellWidth::Single,
            selected: false,
        }
    }
}

/// Shape of the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    /// Full cell.
    Block,
    /// Line under the cell.
    Underline,
    /// Vertical bar.
    Beam,
    /// Outline only, as for an unfocused terminal.
    HollowBlock,
}

/// The cursor as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenCursor {
    /// Viewport row.
    pub row: usize,
    /// Column.
    pub col: usize,
    /// Shape.
    pub style: CursorStyle,
    /// Colour.
    pub color: Rgb,
    /// Over a wide character: spans two cells.
    pub wide: bool,
}

/// Everything needed to draw the terminal once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// Columns.
    pub cols: usize,
    /// Rows.
    pub rows: usize,
    /// Cells, row by row.
    pub cells: Vec<ScreenCell>,
    /// Cursor, absent when hidden or scrolled out of view.
    pub cursor: Option<ScreenCursor>,
    /// Lines scrolled back into the history, 0 at the bottom.
    pub display_offset: usize,
    /// Background of the whole area.
    pub background: Rgb,
    /// Background of selected cells, the palette's.
    pub selection: Rgb,
}

impl Screen {
    /// The cell at `row`, `col`.
    #[must_use]
    pub fn cell(&self, row: usize, col: usize) -> Option<&ScreenCell> {
        (col < self.cols)
            .then(|| self.cells.get(row * self.cols + col))
            .flatten()
    }

    /// Text of a row, spacers skipped, trailing spaces kept.
    #[must_use]
    pub fn row_text(&self, row: usize) -> String {
        (0..self.cols)
            .filter_map(|col| self.cell(row, col))
            .filter(|cell| cell.width != CellWidth::Spacer)
            .flat_map(|cell| std::iter::once(cell.ch).chain(cell.zerowidth.iter().copied()))
            .collect()
    }

    /// The web address shown over cell `row`, `col`, as the C# terminal finds one for
    /// Ctrl+click: `http://` or `https://` and what follows up to a space, `<`, `>`, a quote
    /// or a backquote, the punctuation that ends a sentence left out. Within one row.
    #[must_use]
    pub fn url_at(&self, row: usize, col: usize) -> Option<String> {
        // Each character with the column it starts at; a wide one covers the next too.
        let cells: Vec<(usize, char)> = (0..self.cols)
            .filter_map(|at| self.cell(row, at).map(|cell| (at, cell)))
            .filter(|(_, cell)| cell.width != CellWidth::Spacer)
            .map(|(at, cell)| (at, cell.ch))
            .collect();
        let chars: Vec<char> = cells.iter().map(|(_, ch)| *ch).collect();
        let mut start = 0;
        while start < chars.len() {
            let Some(prefix) = URL_PREFIXES
                .iter()
                .find(|prefix| starts_with_ignoring_case(&chars[start..], prefix))
            else {
                start += 1;
                continue;
            };
            let mut end = start + prefix.chars().count();
            while end < chars.len() && !ends_url(chars[end]) {
                end += 1;
            }
            while end > start && URL_TRAILING.contains(&chars[end - 1]) {
                end -= 1;
            }
            let first = cells[start].0;
            let last = cells.get(end).map_or(self.cols, |(at, _)| *at);
            if (first..last).contains(&col) && end > start + prefix.chars().count() {
                return Some(chars[start..end].iter().collect());
            }
            start = end.max(start + 1);
        }
        None
    }
}

/// What a web address starts with, as the C# terminal's pattern.
const URL_PREFIXES: [&str; 2] = ["http://", "https://"];

/// What ends a web address, as the C# terminal's pattern: a space, `<`, `>`, a quote or a
/// backquote.
fn ends_url(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'' | '`')
}

/// The punctuation left out at the end of a web address, as the C# terminal leaves it.
const URL_TRAILING: [char; 9] = ['.', ',', ';', ':', '!', '?', ')', ']', '}'];

fn starts_with_ignoring_case(chars: &[char], prefix: &str) -> bool {
    prefix.chars().count() <= chars.len()
        && prefix
            .chars()
            .zip(chars)
            .all(|(wanted, ch)| ch.eq_ignore_ascii_case(&wanted))
}

/// Which way [`Terminal::find`] looks through the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindDirection {
    /// Towards the newest lines.
    Down,
    /// Towards the oldest lines.
    Up,
}

/// Where [`Terminal::find`] found a match: its place among all of them, from 1, and how many
/// there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    /// Its place, from 1.
    pub index: usize,
    /// How many matches there are.
    pub total: usize,
}

/// Lines shown above a match found, as the C# terminal scrolls to one.
const FIND_CONTEXT_LINES: i32 = 5;

/// A count of lines as a line offset, saturated.
fn to_i32(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// A cell under the pointer, in viewport coordinates, with the half of the cell it is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPoint {
    /// Viewport row.
    pub row: usize,
    /// Column.
    pub col: usize,
    /// True when the pointer is on the right half of the cell.
    pub right_half: bool,
}

/// How a selection grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    /// Character by character.
    Simple,
    /// Word by word (double click).
    Word,
    /// Line by line (triple click).
    Line,
    /// Rectangle.
    Block,
}

#[derive(Clone, Default)]
struct Listener(Arc<Mutex<VecDeque<Event>>>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        if let Ok(mut queue) = self.0.lock() {
            queue.push_back(event);
        }
    }
}

/// A terminal emulator.
pub struct Terminal {
    term: Term<Listener>,
    parser: Processor<StdSyncHandler>,
    events: Listener,
    palette: Palette,
    cell_pixels: Option<CellPixels>,
    /// The working folder reports, which the emulator ignores, read beside it.
    directory: WorkingDirectoryScanner,
}

impl std::fmt::Debug for Terminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terminal")
            .field("cols", &self.term.columns())
            .field("rows", &self.term.screen_lines())
            .finish_non_exhaustive()
    }
}

impl Terminal {
    /// A terminal of `size`, which must be the size the PTY was created with.
    #[must_use]
    pub fn new(size: GridSize, config: TerminalConfig) -> Self {
        let events = Listener::default();
        let term_config = Config {
            scrolling_history: config.scrollback_lines,
            kitty_keyboard: false,
            osc52: config.clipboard.osc52(),
            ..Config::default()
        };
        Self {
            term: Term::new(term_config, &size.clamped(), events.clone()),
            parser: Processor::new(),
            events,
            palette: config.palette,
            cell_pixels: None,
            directory: WorkingDirectoryScanner::new(),
        }
    }

    /// Processes output from the server.
    pub fn feed(&mut self, bytes: &[u8]) -> FeedOutput {
        self.parser.advance(&mut self.term, bytes);
        let mut output = self.drain_events();
        output.working_directory = self.directory.feed(bytes);
        // While a synchronized update is open, bytes are only buffered.
        output.redraw = self.sync_deadline().is_none();
        output
    }

    /// When an open synchronized update (DEC mode 2026) must be flushed even if the server
    /// never closes it. The caller arms a timer and calls [`Self::flush_sync`] then.
    #[must_use]
    pub fn sync_deadline(&self) -> Option<Instant> {
        self.parser.sync_timeout().sync_timeout()
    }

    /// Ends an open synchronized update and processes what it buffered.
    pub fn flush_sync(&mut self) -> FeedOutput {
        self.parser.stop_sync(&mut self.term);
        let mut output = self.drain_events();
        output.redraw = true;
        output
    }

    /// Changes the grid size. Call it before telling the server, in the same UI update.
    pub fn resize(&mut self, size: GridSize) -> GridSize {
        let size = size.clamped();
        self.term.resize(size);
        size
    }

    /// Records the cell size in pixels, so window size queries can be answered.
    pub fn set_cell_pixels(&mut self, pixels: CellPixels) {
        self.cell_pixels = Some(pixels);
    }

    /// Current grid size.
    #[must_use]
    pub fn size(&self) -> GridSize {
        GridSize {
            cols: self.term.columns(),
            rows: self.term.screen_lines(),
        }
    }

    /// Draws with `palette` from now on; the colours the server set itself stay its own.
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    /// Modes the input encoders need.
    #[must_use]
    pub fn input_mode(&self) -> InputMode {
        InputMode::from(*self.term.mode())
    }

    /// Scrolls the view through the history; positive goes up.
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    /// Back to the bottom of the history.
    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    /// Lines scrolled back.
    #[must_use]
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    fn grid_point(&self, at: CellPoint) -> (Point, Side) {
        let rows = self.term.screen_lines();
        let cols = self.term.columns();
        let viewport = Point::new(at.row.min(rows - 1), Column(at.col.min(cols - 1)));
        let side = if at.right_half {
            Side::Right
        } else {
            Side::Left
        };
        (viewport_to_point(self.display_offset(), viewport), side)
    }

    /// Looks for `query`, whatever its case, in the history and on the screen, as the C#
    /// terminal does: every match is listed; the first look lands on the first match at or
    /// after the top of the view (the last one above it, going up), the next ones step from
    /// the match found (the selection) and wrap around. The match is selected and scrolled
    /// into view, a few lines below its top. Where it is among them all; none when nothing
    /// matches, the selection then cleared.
    pub fn find(&mut self, query: &str, direction: FindDirection) -> Option<Found> {
        let query = query.to_lowercase();
        if query.is_empty() {
            return None;
        }
        let (top, bottom) = (self.term.topmost_line(), self.term.bottommost_line());
        let mut matches = Vec::new();
        let mut line = top;
        while line <= bottom {
            matches.extend(
                self.matches_in(line, &query)
                    .into_iter()
                    .map(|(start, end)| (line, start, end)),
            );
            line += 1;
        }
        let total = matches.len();
        if total == 0 {
            self.term.selection = None;
            return None;
        }
        let view_top = Line(-to_i32(self.display_offset()));
        let current = self
            .term
            .selection
            .as_ref()
            .and_then(|selection| selection.to_range(&self.term))
            .and_then(|range| {
                matches
                    .iter()
                    .position(|(line, start, _)| Point::new(*line, *start) == range.start)
            });
        let index = match (current, direction) {
            (Some(at), FindDirection::Down) => (at + 1) % total,
            (Some(at), FindDirection::Up) => (at + total - 1) % total,
            (None, FindDirection::Down) => matches
                .iter()
                .position(|(line, ..)| *line >= view_top)
                .unwrap_or(0),
            (None, FindDirection::Up) => matches
                .iter()
                .rposition(|(line, ..)| *line < view_top)
                .unwrap_or(total - 1),
        };
        let (line, start, end) = matches[index];
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(line, start), Side::Left);
        selection.update(Point::new(line, end), Side::Right);
        self.term.selection = Some(selection);
        // Scrolling stops at either end of the history by itself.
        let offset = FIND_CONTEXT_LINES - line.0;
        self.term
            .scroll_display(Scroll::Delta(offset - to_i32(self.display_offset())));
        Some(Found {
            index: index + 1,
            total,
        })
    }

    /// The first and last columns of each match of `query`, already lower case, in `line`,
    /// left to right, none overlapping.
    fn matches_in(&self, line: Line, query: &str) -> Vec<(Column, Column)> {
        let row = &self.term.grid()[line];
        let mut text = String::new();
        let mut columns = Vec::new();
        for col in 0..self.term.columns() {
            let cell = &row[Column(col)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            for lower in cell.c.to_lowercase() {
                text.push(lower);
                columns.push(col);
            }
        }
        let length = query.chars().count();
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(at) = text[from..].find(query) {
            let byte = from + at;
            let first = text[..byte].chars().count();
            let last = first + length - 1;
            found.push((Column(columns[first]), Column(columns[last])));
            from = byte + query.len();
        }
        found
    }

    /// Starts a selection at `at`.
    pub fn begin_selection(&mut self, at: CellPoint, kind: SelectionKind) {
        let (point, side) = self.grid_point(at);
        let kind = match kind {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
            SelectionKind::Block => SelectionType::Block,
        };
        self.term.selection = Some(Selection::new(kind, point, side));
    }

    /// Moves the free end of the selection to `to`.
    pub fn extend_selection(&mut self, to: CellPoint) {
        let (point, side) = self.grid_point(to);
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, side);
        }
    }

    /// Removes the selection.
    pub fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    /// Selected text, wrapped lines joined, tabs restored. Text the application hid
    /// (SGR 8) is included: alacritty's selection does not filter it.
    #[must_use]
    pub fn selected_text(&self) -> Option<String> {
        self.term.selection_to_string()
    }

    /// Where the text at `at`, in view, leads when an application tied an address to it
    /// with OSC 8, as a terminal's hyperlink: the text may say something else.
    #[must_use]
    pub fn hyperlink_at(&self, at: CellPoint) -> Option<String> {
        let (point, _) = self.grid_point(at);
        self.term.grid()[point]
            .hyperlink()
            .map(|link| link.uri().to_owned())
    }

    /// The web address shown at `at`, in view, for Ctrl+click: see [`Screen::url_at`].
    #[must_use]
    pub fn url_at(&self, at: CellPoint) -> Option<String> {
        self.snapshot().url_at(at.row, at.col)
    }

    /// Everything needed to draw, into a new screen.
    #[must_use]
    pub fn snapshot(&self) -> Screen {
        let mut screen = Screen {
            cols: 0,
            rows: 0,
            cells: Vec::new(),
            cursor: None,
            display_offset: 0,
            background: self.palette.background,
            selection: self.palette.selection,
        };
        self.snapshot_into(&mut screen);
        screen
    }

    /// Everything needed to draw, into `screen`, reusing its memory.
    pub fn snapshot_into(&self, screen: &mut Screen) {
        let content = self.term.renderable_content();
        let cols = self.term.columns();
        let rows = self.term.screen_lines();
        let offset = content.display_offset;
        screen.cols = cols;
        screen.rows = rows;
        screen.display_offset = offset;
        screen.background = self.color(NamedColor::Background as usize);
        screen.selection = self.palette.selection;
        screen.cells.clear();
        screen
            .cells
            .resize(cols * rows, ScreenCell::blank(&self.palette));

        let cursor = content.cursor;
        for indexed in content.display_iter {
            let Some(viewport) = point_to_viewport(offset, indexed.point) else {
                continue;
            };
            let Some(slot) = screen
                .cells
                .get_mut(viewport.line * cols + viewport.column.0)
            else {
                continue;
            };
            let selected = content
                .selection
                .is_some_and(|range| range.contains_cell(&indexed, cursor.point, cursor.shape));
            *slot = self.render_cell(indexed.cell, selected);
        }

        screen.cursor = self.render_cursor(cursor.shape, cursor.point, offset);
    }

    fn render_cursor(
        &self,
        shape: CursorShape,
        point: Point,
        offset: usize,
    ) -> Option<ScreenCursor> {
        let style = match shape {
            CursorShape::Hidden => return None,
            CursorShape::Block => CursorStyle::Block,
            CursorShape::Underline => CursorStyle::Underline,
            CursorShape::Beam => CursorStyle::Beam,
            CursorShape::HollowBlock => CursorStyle::HollowBlock,
        };
        let viewport = point_to_viewport(offset, point)?;
        if viewport.line >= self.term.screen_lines() {
            return None;
        }
        Some(ScreenCursor {
            row: viewport.line,
            col: viewport.column.0,
            style,
            color: self.color(CURSOR_COLOR_SLOT),
            wide: self.term.grid()[point].flags.contains(Flags::WIDE_CHAR),
        })
    }

    /// Colour of a slot: the server's override when it set one, else the palette.
    fn color(&self, slot: usize) -> Rgb {
        self.term.colors()[slot].unwrap_or_else(|| self.palette.slot(slot))
    }

    fn resolve(&self, color: Color, flags: Flags, foreground: bool) -> Rgb {
        let dimmed = foreground && flags.contains(Flags::DIM);
        match color {
            Color::Named(named) => {
                let bold_bright = foreground
                    && flags.contains(Flags::BOLD)
                    && (named as usize) < NamedColor::BrightBlack as usize;
                let named = if bold_bright {
                    named.to_bright()
                } else {
                    named
                };
                let named = if dimmed { named.to_dim() } else { named };
                self.color(named as usize)
            }
            Color::Indexed(index) => {
                let rgb = self.color(usize::from(index));
                if dimmed { dim(rgb) } else { rgb }
            }
            Color::Spec(rgb) => {
                if dimmed {
                    dim(rgb)
                } else {
                    rgb
                }
            }
        }
    }

    fn render_cell(&self, cell: &Cell, selected: bool) -> ScreenCell {
        let flags = cell.flags;
        let mut fg = self.resolve(cell.fg, flags, true);
        let mut bg = self.resolve(cell.bg, flags, false);
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        let width = if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            CellWidth::Spacer
        } else if flags.contains(Flags::WIDE_CHAR) {
            CellWidth::Wide
        } else {
            CellWidth::Single
        };
        let ch = if width == CellWidth::Spacer || cell.c == '\t' {
            ' '
        } else {
            cell.c
        };
        let underline = if flags.contains(Flags::DOUBLE_UNDERLINE) {
            Underline::Double
        } else if flags.contains(Flags::UNDERCURL) {
            Underline::Curl
        } else if flags.contains(Flags::DOTTED_UNDERLINE) {
            Underline::Dotted
        } else if flags.contains(Flags::DASHED_UNDERLINE) {
            Underline::Dashed
        } else if flags.contains(Flags::UNDERLINE) {
            Underline::Single
        } else if cell.hyperlink().is_some() {
            // Text an OSC 8 address is tied to: told apart from text the application
            // underlined itself.
            Underline::Dashed
        } else {
            Underline::None
        };
        ScreenCell {
            ch,
            zerowidth: cell.zerowidth().map(<[char]>::to_vec).unwrap_or_default(),
            fg,
            bg,
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline,
            underline_color: cell
                .underline_color()
                .map(|color| self.resolve(color, Flags::empty(), true)),
            strikeout: flags.contains(Flags::STRIKEOUT),
            width,
            selected,
        }
    }

    fn drain_events(&mut self) -> FeedOutput {
        let events: Vec<Event> = match self.events.0.lock() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => Vec::new(),
        };
        let mut output = FeedOutput::default();
        for event in events {
            match event {
                Event::PtyWrite(text) => output.replies.extend_from_slice(text.as_bytes()),
                Event::ColorRequest(slot, format) => {
                    output
                        .replies
                        .extend_from_slice(format(self.color(slot)).as_bytes());
                }
                Event::TextAreaSizeRequest(format) => {
                    if let Some(size) = self.window_size() {
                        output.replies.extend_from_slice(format(size).as_bytes());
                    }
                }
                Event::ClipboardStore(_, text) => output.clipboard = Some(text),
                Event::Title(title) => output.title = Some(TitleChange::Set(title)),
                Event::ResetTitle => output.title = Some(TitleChange::Reset),
                Event::Bell => output.bell = true,
                // Load is refused by the Osc52 configuration; never answered here either.
                Event::ClipboardLoad(..)
                | Event::MouseCursorDirty
                | Event::CursorBlinkingChange
                | Event::Wakeup
                | Event::Exit
                | Event::ChildExit(_) => {}
            }
        }
        output
    }

    fn window_size(&self) -> Option<WindowSize> {
        let pixels = self.cell_pixels?;
        Some(WindowSize {
            num_lines: u16::try_from(self.term.screen_lines()).ok()?,
            num_cols: u16::try_from(self.term.columns()).ok()?,
            cell_width: pixels.width,
            cell_height: pixels.height,
        })
    }
}
