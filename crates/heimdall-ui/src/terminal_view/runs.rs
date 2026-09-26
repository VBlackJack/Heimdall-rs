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

//! A screen row cut into what is drawn in one call.
//!
//! ASCII text of one style is drawn as one run: the embedded font gives every ASCII glyph
//! the cell's width, so a run stays on the grid. Any other character is drawn alone at its
//! own cell, so a fallback glyph of another width cannot shift the rest of the row.

use heimdall_term::{CellWidth, Rgb, ScreenCell, Underline};

/// Text drawn in one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    /// First column.
    pub col: usize,
    /// Columns covered.
    pub cells: usize,
    /// Characters.
    pub content: String,
    /// Colour.
    pub fg: Rgb,
    /// Bold face.
    pub bold: bool,
    /// Italic face.
    pub italic: bool,
    /// Only ASCII: basic shaping suffices.
    pub ascii: bool,
}

/// A stretch of cells sharing a background different from the screen's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundRun {
    /// First column.
    pub col: usize,
    /// Columns covered.
    pub cells: usize,
    /// Colour.
    pub color: Rgb,
}

/// An underline or strike line over a stretch of cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRun {
    /// First column.
    pub col: usize,
    /// Columns covered.
    pub cells: usize,
    /// Colour.
    pub color: Rgb,
    /// Underline (true) or strikeout (false).
    pub underline: bool,
}

fn span(cell: &ScreenCell) -> usize {
    if cell.width == CellWidth::Wide { 2 } else { 1 }
}

/// Backgrounds to paint over the screen background; selected cells use `selection`.
#[must_use]
pub fn backgrounds(row: &[ScreenCell], screen: Rgb, selection: Rgb) -> Vec<BackgroundRun> {
    let mut runs: Vec<BackgroundRun> = Vec::new();
    for (col, cell) in row.iter().enumerate() {
        let color = if cell.selected { selection } else { cell.bg };
        if color == screen {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.color == color && run.col + run.cells == col => run.cells += 1,
            _ => runs.push(BackgroundRun {
                col,
                cells: 1,
                color,
            }),
        }
    }
    runs
}

/// Text to draw; blanks and spacers produce nothing.
#[must_use]
pub fn texts(row: &[ScreenCell]) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut extendable = false;
    for (col, cell) in row.iter().enumerate() {
        let blank = cell.ch == ' ' && cell.zerowidth.is_empty();
        if cell.width == CellWidth::Spacer || blank {
            extendable = false;
            continue;
        }
        let simple = cell.ch.is_ascii() && cell.zerowidth.is_empty() && span(cell) == 1;
        if simple
            && extendable
            && let Some(run) = runs.last_mut()
            && run.ascii
            && run.fg == cell.fg
            && run.bold == cell.bold
            && run.italic == cell.italic
            && run.col + run.cells == col
        {
            run.content.push(cell.ch);
            run.cells += 1;
            continue;
        }
        let mut content = String::from(cell.ch);
        content.extend(cell.zerowidth.iter());
        runs.push(TextRun {
            col,
            cells: span(cell),
            content,
            fg: cell.fg,
            bold: cell.bold,
            italic: cell.italic,
            ascii: simple,
        });
        extendable = simple;
    }
    runs
}

/// Underlines and strike lines.
#[must_use]
pub fn lines(row: &[ScreenCell]) -> Vec<LineRun> {
    let mut runs: Vec<LineRun> = Vec::new();
    for (col, cell) in row.iter().enumerate() {
        let mut push = |underline: bool, color: Rgb| match runs.last_mut() {
            Some(run)
                if run.underline == underline
                    && run.color == color
                    && run.col + run.cells == col =>
            {
                run.cells += 1;
            }
            _ => runs.push(LineRun {
                col,
                cells: 1,
                color,
                underline,
            }),
        };
        if cell.underline != Underline::None {
            push(true, cell.underline_color.unwrap_or(cell.fg));
        }
        if cell.strikeout {
            push(false, cell.fg);
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use heimdall_term::{GridSize, Rgb, Terminal, TerminalConfig};

    use super::{backgrounds, texts};

    fn row(bytes: &[u8]) -> Vec<heimdall_term::ScreenCell> {
        let mut terminal = Terminal::new(GridSize { cols: 20, rows: 1 }, TerminalConfig::default());
        terminal.feed(bytes);
        terminal.snapshot().cells
    }

    #[test]
    fn ascii_of_one_style_is_one_run_and_spaces_split_runs() {
        let runs = texts(&row(b"ls -la"));
        let shown: Vec<(usize, &str)> = runs.iter().map(|r| (r.col, r.content.as_str())).collect();
        assert_eq!(shown, vec![(0, "ls"), (3, "-la")]);
    }

    #[test]
    fn a_style_change_starts_a_new_run() {
        let runs = texts(&row(b"ab\x1b[1mcd"));
        assert_eq!(runs.len(), 2);
        assert!(runs[1].bold);
    }

    #[test]
    fn wide_and_combining_characters_are_drawn_alone_at_their_cell() {
        let runs = texts(&row("a\u{4e2d}b e\u{301}x".as_bytes()));
        let shown: Vec<(usize, usize, &str)> = runs
            .iter()
            .map(|r| (r.col, r.cells, r.content.as_str()))
            .collect();
        assert_eq!(
            shown,
            vec![
                (0, 1, "a"),
                (1, 2, "\u{4e2d}"),
                (3, 1, "b"),
                (5, 1, "e\u{301}"),
                (6, 1, "x")
            ]
        );
    }

    #[test]
    fn backgrounds_merge_and_skip_the_screen_colour() {
        let screen = Rgb { r: 0, g: 0, b: 0 };
        let red = Rgb { r: 255, g: 0, b: 0 };
        let cells = row(b"\x1b[48;2;255;0;0mab\x1b[0mc");
        let runs = backgrounds(&cells, heimdall_term::Palette::dracula().background, screen);
        assert_eq!(runs.len(), 1);
        assert_eq!((runs[0].col, runs[0].cells, runs[0].color), (0, 2, red));
    }
}
