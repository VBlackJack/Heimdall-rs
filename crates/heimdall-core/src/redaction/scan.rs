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

//! The scanning machinery shared by every rule: the per-rule time budget, a pattern with the
//! look-behind and look-ahead checks the `regex` crate cannot express, chunked scanning and the
//! application of edits.
//!
//! The C# `RedactionApply` relies on a backtracking engine with a match timeout. Here every
//! pattern runs on the linear-time `regex` engine, so a timeout cannot come from one pathological
//! match; the budget is checked between matches and between chunks instead, and a rule that runs
//! out of it fails the whole redaction closed.

use std::time::{Duration, Instant};

use regex::{CaptureLocations, Regex, RegexBuilder};

use super::RedactionError;

/// Target chunk size for single-line rules; a chunk always ends on a line boundary
/// (C# `RedactionApply.ChunkChars`).
pub(super) const CHUNK_BYTES: usize = 64 * 1024;

/// Compiled-program size limit of one pattern: the bounded repetitions of a Unicode class
/// (`{1,4096}` and `{8,8192}`) are large once expanded.
const REGEX_SIZE_LIMIT_BYTES: usize = 256 * 1024 * 1024;

/// Lazy DFA cache of one pattern: a small cache would make the engine fall back to the much
/// slower NFA simulation on a large buffer.
const REGEX_DFA_CACHE_BYTES: usize = 128 * 1024 * 1024;

/// The time a rule has left. Created per rule, so the budget is per rule and not per buffer.
pub(super) struct Budget {
    started: Instant,
    limit: Duration,
    rule: &'static str,
}

impl Budget {
    pub(super) fn new(rule: &'static str, limit: Duration) -> Self {
        Self {
            started: Instant::now(),
            limit,
            rule,
        }
    }

    /// Fails closed once the rule has used its time.
    pub(super) fn check(&self) -> Result<(), RedactionError> {
        if self.started.elapsed() >= self.limit {
            return Err(RedactionError::Timeout { rule: self.rule });
        }
        Ok(())
    }
}

/// One match, in byte offsets of the haystack it was found in.
#[derive(Debug, Clone, Copy)]
pub(super) struct Found {
    pub start: usize,
    pub end: usize,
    /// The span to mask: the first participating group whose name starts with `v`.
    pub value: Option<(usize, usize)>,
}

/// An extra condition on a match, standing for a look-behind or a look-ahead. Receives the
/// haystack and the match. A match that fails it is skipped and the search resumes one
/// character after its start, which is what a backtracking engine does at that position.
pub(super) type Guard = fn(&str, &Found) -> bool;

pub(super) struct Pattern {
    regex: Regex,
    value_groups: Vec<usize>,
    guard: Option<Guard>,
}

impl Pattern {
    /// Compiles a static pattern. A pattern that does not compile is a programming error caught
    /// by the tests, never a runtime input.
    pub(super) fn new(source: &str, guard: Option<Guard>) -> Self {
        let regex = RegexBuilder::new(source)
            .size_limit(REGEX_SIZE_LIMIT_BYTES)
            .dfa_size_limit(REGEX_DFA_CACHE_BYTES)
            .build()
            .expect("a static redaction pattern compiles");
        let value_groups = regex
            .capture_names()
            .enumerate()
            .filter(|(_, name)| name.is_some_and(|n| n.starts_with('v')))
            .map(|(index, _)| index)
            .collect();
        Self {
            regex,
            value_groups,
            guard,
        }
    }

    /// The first match at or after `from`, whose guard passes.
    pub(super) fn find_from(
        &self,
        hay: &str,
        from: usize,
        budget: &Budget,
    ) -> Result<Option<Found>, RedactionError> {
        let mut locations = self.regex.capture_locations();
        self.find_with(&mut locations, hay, from, budget)
    }

    fn find_with(
        &self,
        locations: &mut CaptureLocations,
        hay: &str,
        from: usize,
        budget: &Budget,
    ) -> Result<Option<Found>, RedactionError> {
        let mut at = from;
        while at <= hay.len() {
            budget.check()?;
            let Some(whole) = self.regex.captures_read_at(locations, hay, at) else {
                return Ok(None);
            };
            let found = Found {
                start: whole.start(),
                end: whole.end(),
                value: self
                    .value_groups
                    .iter()
                    .find_map(|&index| locations.get(index)),
            };
            if self.guard.is_none_or(|guard| guard(hay, &found)) {
                return Ok(Some(found));
            }
            at = next_boundary(hay, found.start);
        }
        Ok(None)
    }

    /// Every non-overlapping match of the haystack, in order.
    pub(super) fn find_all(
        &self,
        hay: &str,
        budget: &Budget,
    ) -> Result<Vec<Found>, RedactionError> {
        let mut locations = self.regex.capture_locations();
        let mut all = Vec::new();
        let mut at = 0;
        while let Some(found) = self.find_with(&mut locations, hay, at, budget)? {
            at = if found.end > found.start {
                found.end
            } else {
                next_boundary(hay, found.end)
            };
            all.push(found);
        }
        Ok(all)
    }

    /// Every match of the text, chunk by chunk (each chunk ends after a `\n`, so no match
    /// crosses one), in offsets of the whole text.
    pub(super) fn scan(&self, text: &str, budget: &Budget) -> Result<Vec<Found>, RedactionError> {
        let mut all = Vec::new();
        let mut chunk_start = 0;
        while chunk_start < text.len() {
            let chunk_end = next_chunk_end(text, chunk_start);
            for mut found in self.find_all(&text[chunk_start..chunk_end], budget)? {
                found.start += chunk_start;
                found.end += chunk_start;
                found.value = found
                    .value
                    .map(|(start, end)| (start + chunk_start, end + chunk_start));
                all.push(found);
            }
            chunk_start = chunk_end;
        }
        Ok(all)
    }
}

/// The offset of the character after the one at `index`.
fn next_boundary(hay: &str, index: usize) -> usize {
    hay[index..]
        .chars()
        .next()
        .map_or(hay.len() + 1, |c| index + c.len_utf8())
}

/// The end of the chunk starting at `start`: after a `\n`, and never before a continuation line
/// (one that starts with a blank), so a folded header is not cut in two.
fn next_chunk_end(text: &str, start: usize) -> usize {
    let target = start + CHUNK_BYTES;
    if target >= text.len() {
        return text.len();
    }
    let bytes = text.as_bytes();
    let mut end = bytes[target..]
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(text.len(), |offset| target + offset + 1);
    while end < text.len() && matches!(bytes[end], b' ' | b'\t') {
        end = bytes[end..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(text.len(), |offset| end + offset + 1);
    }
    end
}

/// The character before `index`, if any.
pub(super) fn char_before(hay: &str, index: usize) -> Option<char> {
    hay[..index].chars().next_back()
}

/// The character at `index`, if any.
pub(super) fn char_at(hay: &str, index: usize) -> Option<char> {
    hay[index..].chars().next()
}

/// A half-open byte range of the input replaced by `replacement`.
#[derive(Debug)]
pub(super) struct Edit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// A logical line: `start` inclusive, `end` exclusive, without `\r` and `\n`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Line {
    pub start: usize,
    pub end: usize,
}

pub(super) fn split_lines(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let newline = text.as_bytes()[start..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|offset| start + offset);
        let end = newline.unwrap_or(text.len());
        let content_end = if end > start && text.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        lines.push(Line {
            start,
            end: content_end,
        });
        match newline {
            Some(position) => start = position + 1,
            None => break,
        }
    }
    lines
}

/// Applies edits. Edits that overlap are merged into one that covers their union and keeps the
/// first one's replacement (the C# throws on overlapping edits), so no part of a masked span is
/// ever left in the text, whatever order the rules produced the edits in.
pub(super) fn apply_edits(text: &str, edits: &[Edit]) -> String {
    if edits.is_empty() {
        return text.to_owned();
    }
    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by_key(|edit| (edit.start, edit.end));
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut current: Option<(usize, usize, &str)> = None;
    for edit in ordered {
        match &mut current {
            Some((_, end, _)) if edit.start < *end => *end = (*end).max(edit.end),
            _ => {
                if let Some((start, end, replacement)) = current.take() {
                    out.push_str(&text[copied..start]);
                    out.push_str(replacement);
                    copied = end;
                }
                current = Some((edit.start, edit.end, edit.replacement.as_str()));
            }
        }
    }
    if let Some((start, end, replacement)) = current {
        out.push_str(&text[copied..start]);
        out.push_str(replacement);
        copied = end;
    }
    out.push_str(&text[copied..]);
    out
}
