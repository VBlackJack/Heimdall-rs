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

//! Two texts compared as the C# `DiffEngine` (`Heimdall.Core/Matching/DiffEngine.cs`)
//! compares them: line by line, `\r\n` and `\r` read as `\n`, lines compared with their
//! spaces collapsed or their case ignored when asked; each line of the result unchanged,
//! added or removed, the removed lines of a change before the added ones. A line removed
//! then added again is compared word by word, a word being a run of spaces or of the rest.
//!
//! The lines are compared by `similar`'s Myers diff, which finds as many unchanged lines as
//! the C#'s longest common subsequence without its table of every pair of lines; where two
//! answers are as short, the lines it keeps may differ. The words of a line are compared as
//! the C#'s `WordDiff` compares them, its table and its choice between equal answers kept,
//! as its tests pin them: a line is short.

use similar::{Algorithm, DiffTag};

/// Most lines a side may have, as the C# `DiffEngine.DefaultMaxLineCount`.
pub const DEFAULT_MAX_LINE_COUNT: usize = 10_000;

/// The options of a comparison, as the C# `DiffOptions`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffOptions {
    /// Lines compared trimmed, their runs of spaces as one.
    pub ignore_whitespace: bool,
    /// Lines compared whatever their case.
    pub ignore_case: bool,
    /// Most lines a side may have; [`DEFAULT_MAX_LINE_COUNT`] when `None`.
    pub max_line_count: Option<usize>,
}

impl DiffOptions {
    /// Most lines a side may have.
    #[must_use]
    pub fn effective_max_line_count(&self) -> usize {
        self.max_line_count.unwrap_or(DEFAULT_MAX_LINE_COUNT)
    }
}

/// What became of a line, as the C# `DiffLineKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    /// In both texts.
    Unchanged,
    /// In the modified text only.
    Added,
    /// In the original text only.
    Removed,
}

/// A line of the result, as the C# `DiffLine`: an unchanged line as the original has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// What became of it.
    pub kind: DiffLineKind,
    /// Its text.
    pub text: String,
}

/// What a comparison gave, as the C# `TextDiffResult` and its `DiffStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextDiff {
    /// The lines, in order, and how many of each kind.
    Success(DiffLines),
    /// A side has more lines than allowed.
    InputTooLarge,
}

/// The lines of a comparison and their counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffLines {
    /// The lines, in order.
    pub lines: Vec<DiffLine>,
    /// Lines added.
    pub added: usize,
    /// Lines removed.
    pub removed: usize,
    /// Lines unchanged.
    pub unchanged: usize,
}

/// A part of a line compared word by word, as the C# `WordSegment`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSegment {
    /// Its text.
    pub text: String,
    /// Whether it is not in the other line.
    pub changed: bool,
}

impl WordSegment {
    /// A part of `text`, changed or not.
    #[must_use]
    pub fn new(text: &str, changed: bool) -> Self {
        Self {
            text: text.to_owned(),
            changed,
        }
    }
}

/// Two lines compared word by word, as the C# `WordDiffResult`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordDiff {
    /// The original line's parts.
    pub old: Vec<WordSegment>,
    /// The modified line's parts.
    pub new: Vec<WordSegment>,
}

/// `original` and `modified` compared line by line, as the C# `DiffEngine.Diff`
/// (`DiffEngine.cs:61-96`).
#[must_use]
pub fn diff(original: &str, modified: &str, options: DiffOptions) -> TextDiff {
    let original_lines = split_lines(original);
    let modified_lines = split_lines(modified);
    let max = options.effective_max_line_count();
    if original_lines.len() > max || modified_lines.len() > max {
        return TextDiff::InputTooLarge;
    }
    let normalized_original = normalize_lines(&original_lines, options);
    let normalized_modified = normalize_lines(&modified_lines, options);
    let ops =
        similar::capture_diff_slices(Algorithm::Myers, &normalized_original, &normalized_modified);
    let mut result = DiffLines::default();
    // The lines of a change, removed and added, written out when the change ends: the
    // removed ones first, as the C#'s backtracking leaves them.
    let mut removed: Vec<DiffLine> = Vec::new();
    let mut added: Vec<DiffLine> = Vec::new();
    for op in ops {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        match tag {
            DiffTag::Equal => {
                flush_change(&mut result, &mut removed, &mut added);
                for index in old_range {
                    result.unchanged += 1;
                    result.lines.push(DiffLine {
                        kind: DiffLineKind::Unchanged,
                        text: original_lines[index].to_owned(),
                    });
                }
            }
            DiffTag::Delete | DiffTag::Insert | DiffTag::Replace => {
                removed.extend(old_range.map(|index| DiffLine {
                    kind: DiffLineKind::Removed,
                    text: original_lines[index].to_owned(),
                }));
                added.extend(new_range.map(|index| DiffLine {
                    kind: DiffLineKind::Added,
                    text: modified_lines[index].to_owned(),
                }));
            }
        }
    }
    flush_change(&mut result, &mut removed, &mut added);
    TextDiff::Success(result)
}

/// The change gathered, its removed lines then its added ones, written to `result`.
fn flush_change(result: &mut DiffLines, removed: &mut Vec<DiffLine>, added: &mut Vec<DiffLine>) {
    result.removed += removed.len();
    result.added += added.len();
    result.lines.append(removed);
    result.lines.append(added);
}

/// The lines of `text`, as the C# `SplitLines`: none for an empty text, an empty last line
/// after a final line break.
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(['\r', '\n']) {
        lines.push(&rest[..at]);
        let skip = if rest[at..].starts_with("\r\n") { 2 } else { 1 };
        rest = &rest[at + skip..];
    }
    lines.push(rest);
    lines
}

/// `lines` as they are compared, as the C# `NormalizeLine`.
fn normalize_lines(lines: &[&str], options: DiffOptions) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            let mut normalized = if options.ignore_whitespace {
                line.split_whitespace().collect::<Vec<&str>>().join(" ")
            } else {
                (*line).to_owned()
            };
            if options.ignore_case {
                normalized = normalized
                    .chars()
                    .map(|c| {
                        let mut lower = c.to_lowercase();
                        match (lower.next(), lower.next()) {
                            (Some(single), None) => single,
                            _ => c,
                        }
                    })
                    .collect();
            }
            normalized
        })
        .collect()
}

/// `old_line` and `new_line` compared word by word, as the C# `DiffEngine.WordDiff`
/// (`DiffEngine.cs:98-107`).
#[must_use]
pub fn word_diff(old_line: &str, new_line: &str) -> WordDiff {
    let old_tokens = tokenize(old_line);
    let new_tokens = tokenize(new_line);
    let (old_kept, new_kept) = common_tokens(&old_tokens, &new_tokens);
    WordDiff {
        old: merge(&old_tokens, &old_kept),
        new: merge(&new_tokens, &new_kept),
    }
}

/// The words and runs of spaces of `line`, as the C# `TokenizeLine`.
fn tokenize(line: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut space = None;
    for (at, c) in line.char_indices() {
        let is_space = c.is_whitespace();
        if space.is_some_and(|run| run != is_space) {
            tokens.push(&line[start..at]);
            start = at;
        }
        space = Some(is_space);
    }
    if start < line.len() {
        tokens.push(&line[start..]);
    }
    tokens
}

/// Which tokens of each side are in their longest common subsequence, as the C#
/// `ComputeWordInLcs` (`DiffEngine.cs:224-266`) finds and walks it back.
fn common_tokens(old: &[&str], new: &[&str]) -> (Vec<bool>, Vec<bool>) {
    let width = new.len() + 1;
    let mut table = vec![0_usize; (old.len() + 1) * width];
    for i in 1..=old.len() {
        for j in 1..=new.len() {
            table[i * width + j] = if old[i - 1] == new[j - 1] {
                table[(i - 1) * width + j - 1] + 1
            } else {
                table[(i - 1) * width + j].max(table[i * width + j - 1])
            };
        }
    }
    let mut old_kept = vec![false; old.len()];
    let mut new_kept = vec![false; new.len()];
    let (mut x, mut y) = (old.len(), new.len());
    while x > 0 && y > 0 {
        if old[x - 1] == new[y - 1] {
            old_kept[x - 1] = true;
            new_kept[y - 1] = true;
            x -= 1;
            y -= 1;
        } else if table[(x - 1) * width + y] >= table[x * width + y - 1] {
            x -= 1;
        } else {
            y -= 1;
        }
    }
    (old_kept, new_kept)
}

/// `tokens` joined into parts changed or not, as the C# `MergeSegments`.
fn merge(tokens: &[&str], kept: &[bool]) -> Vec<WordSegment> {
    let mut segments: Vec<WordSegment> = Vec::new();
    for (token, kept) in tokens.iter().zip(kept) {
        let changed = !kept;
        match segments.last_mut() {
            Some(last) if last.changed == changed => last.text.push_str(token),
            _ => segments.push(WordSegment::new(token, changed)),
        }
    }
    segments
}
