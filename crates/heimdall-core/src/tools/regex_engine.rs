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

//! A pattern tried on a text as the C# `RegexEngine` (`Heimdall.Core/Matching/RegexEngine.cs`)
//! tries it: every match, where it starts and how long it is, its groups but the whole
//! match, a group that took no part said so; places and lengths counted in UTF-16 units, as
//! .NET counts them.
//!
//! The engine is `fancy-regex`, not .NET's, and the `regex` crate under it for a pattern that
//! needs no backtracking. Its syntax is .NET's for what both have:
//! classes, quantifiers lazy or not, anchors, `\b`, `\d`, `\w`, `\s` (Unicode, as .NET's),
//! groups numbered or named `(?<name>...)`, inline flags, look-ahead and look-behind,
//! backreferences (`\1`, `\k<name>`), atomic groups `(?>...)` and conditionals
//! `(?(1)...|...)`. What .NET has and it has not is refused, as .NET refuses a pattern it
//! cannot read, and named so the user sees why: a look-behind whose length varies, and
//! balancing groups `(?<open-close>...)`.
//!
//! A pattern with look-arounds, backreferences, atomic groups or conditionals backtracks, as
//! .NET's engine does, and can take forever on a text. The C# stops a test after its
//! one-second timeout (`RegexEngine.cs:51`, `:65`, `:98-101`) and says it timed out. Here
//! such a pattern is searched on a thread of its own, which the caller waits for at most
//! [`DEFAULT_TIMEOUT`]: past it, the test has timed out. The thread stops by itself at its
//! next match, its clock past the same limit, or when a search takes more than
//! [`BACKTRACK_LIMIT`] steps back, which also ends the test as timed out; what it finds
//! afterwards is dropped. A pattern the `regex` crate reads needs no backtracking: it is
//! searched in linear time where it is asked, the clock read between its matches.
//!
//! Matches follow one another as .NET's do: the next search starts where a match ended, one
//! character further after an empty one.
//!
//! Groups are numbered as .NET numbers them: the unnamed ones first, in order, then the
//! named ones.

use std::ops::Range;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use fancy_regex::{CompileError, Error, RegexBuilder};

/// How long a test may run, as the C# `RegexEngine.DefaultTimeout`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);

/// Most steps back one search may take before it is stopped as timed out: `fancy-regex`'s
/// own default, said here as the bound the timeout relies on.
pub const BACKTRACK_LIMIT: usize = 1_000_000;

/// The options of a test, as the C# `RegexOptions` the tool sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RegexOptions {
    /// Letters match whatever their case, as `RegexOptions.IgnoreCase`.
    pub ignore_case: bool,
    /// `^` and `$` match at each line's start and end, as `RegexOptions.Multiline`.
    pub multiline: bool,
    /// `.` matches a line break too, as `RegexOptions.Singleline`.
    pub singleline: bool,
}

/// A construct of .NET's syntax the engine does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotnetConstruct {
    /// A look-behind whose length varies, as `(?<=a+)`.
    VariableLookBehind,
    /// A balancing group, `(?<open-close>...)` or `(?<-close>...)`.
    BalancingGroup,
}

/// Why a pattern was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidPattern {
    /// It uses a construct of .NET's the engine does not have.
    Unsupported(DotnetConstruct),
    /// It is not a pattern: what the engine says of it, in English as .NET's message is.
    Syntax(String),
}

/// A group of a match, as the C# `RegexGroupInfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexGroup {
    /// Its number, as .NET numbers it, from 1.
    pub number: usize,
    /// Its name, or its number when it has none.
    pub name: String,
    /// Where it starts, in UTF-16 units; `None` when it took no part, as the C#'s -1.
    pub start: Option<usize>,
    /// Its length, in UTF-16 units.
    pub length: usize,
    /// What it matched; empty when it took no part.
    pub value: String,
    /// Whether it has a name.
    pub named: bool,
}

/// A match, as the C# `RegexMatchInfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexMatch {
    /// Where it starts, in UTF-16 units.
    pub index: usize,
    /// Its length, in UTF-16 units.
    pub length: usize,
    /// Where it starts and ends in the text's bytes.
    pub bytes: std::ops::Range<usize>,
    /// What it matched.
    pub value: String,
    /// Its groups, the whole match left out.
    pub groups: Vec<RegexGroup>,
}

/// What a test gave, as the C# `RegexTestResult` and its `RegexTestStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegexTest {
    /// No pattern was given.
    EmptyPattern,
    /// The pattern was refused.
    InvalidPattern(InvalidPattern),
    /// The test took longer than allowed, as the C#'s `RegexMatchTimeoutException`.
    MatchTimeout,
    /// The matches, in order.
    Success(Vec<RegexMatch>),
}

/// `pattern` tried on `input` with `options` within [`DEFAULT_TIMEOUT`], as the C#
/// `RegexEngine.Test` (`RegexEngine.cs:53-102`).
#[must_use]
pub fn test(pattern: &str, input: &str, options: RegexOptions) -> RegexTest {
    test_within(pattern, input, options, DEFAULT_TIMEOUT)
}

/// `pattern` tried on `input` with `options`, stopped as timed out past `timeout`, as the C#
/// `RegexEngine.Test` with its `timeout` argument.
#[must_use]
pub fn test_within(
    pattern: &str,
    input: &str,
    options: RegexOptions,
    timeout: Duration,
) -> RegexTest {
    if pattern.is_empty() {
        return RegexTest::EmptyPattern;
    }
    let started = Instant::now();
    // The pattern read by `fancy-regex` first: it says what is wrong with it, as .NET would.
    let fancy = match fancy_builder(pattern, options).build() {
        Ok(regex) => regex,
        Err(error) => return RegexTest::InvalidPattern(refusal(pattern, &error)),
    };
    let names: Vec<Option<String>> = fancy
        .capture_names()
        .map(|name| name.map(str::to_owned))
        .collect();
    let engine = match regex::RegexBuilder::new(pattern)
        .case_insensitive(options.ignore_case)
        .multi_line(options.multiline)
        .dot_matches_new_line(options.singleline)
        .build()
    {
        Ok(linear) => Engine::Linear(linear),
        Err(_) => Engine::Backtracking(fancy),
    };
    let order = dotnet_order(&names);
    let deadline = started + timeout;
    match engine {
        Engine::Linear(linear) => matches_until(input, &order, deadline, |at| {
            Ok(linear_find(&linear, input, at))
        }),
        Engine::Backtracking(fancy) => {
            // Searched apart, the caller waiting no longer than the timeout.
            let (sender, receiver) = mpsc::channel();
            let text = input.to_owned();
            std::thread::spawn(move || {
                let found =
                    matches_until(&text, &order, deadline, |at| fancy_find(&fancy, &text, at));
                // The caller may have stopped waiting: what was found is then dropped.
                let _ = sender.send(found);
            });
            receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or(RegexTest::MatchTimeout)
        }
    }
}

/// The groups' places of a match, by the engine's numbers.
type Found = Vec<Option<Range<usize>>>;

/// The matches of `input`, each found by `find` from a byte on, in .NET's `order` of
/// groups, as .NET follows them: the next search where a match ended, one character
/// further after an empty one; timed out past `deadline` or when `find` is stopped.
fn matches_until(
    input: &str,
    order: &[(usize, Option<String>)],
    deadline: Instant,
    mut find: impl FnMut(usize) -> Result<Option<Found>, Timeout>,
) -> RegexTest {
    let mut counter = Utf16Counter::default();
    let mut matches = Vec::new();
    let mut at = 0;
    while at <= input.len() {
        if Instant::now() > deadline {
            return RegexTest::MatchTimeout;
        }
        let Ok(found) = find(at) else {
            return RegexTest::MatchTimeout;
        };
        let Some(Some(whole)) = found.as_ref().and_then(|groups| groups.first().cloned()) else {
            break;
        };
        let groups = found.unwrap_or_default();
        matches.push(found_match(input, &whole, &groups, order, &mut counter));
        at = if whole.end > whole.start {
            whole.end
        } else {
            next_char(input, whole.end)
        };
    }
    RegexTest::Success(matches)
}

/// The match of `regex` in `input` from byte `at` on.
fn linear_find(regex: &regex::Regex, input: &str, at: usize) -> Option<Found> {
    regex.captures_at(input, at).map(|captures| {
        captures
            .iter()
            .map(|group| group.map(|group| group.range()))
            .collect()
    })
}

/// The match of `regex` in `input` from byte `at` on; stopped past its steps back.
fn fancy_find(
    regex: &fancy_regex::Regex,
    input: &str,
    at: usize,
) -> Result<Option<Found>, Timeout> {
    regex
        .captures_from_pos(input, at)
        .map(|found| {
            found.map(|captures| {
                captures
                    .iter()
                    .map(|group| group.map(|group| group.range()))
                    .collect()
            })
        })
        .map_err(|_| Timeout)
}

/// A builder of `pattern` with `options`, its steps back bounded.
fn fancy_builder(pattern: &str, options: RegexOptions) -> RegexBuilder {
    let mut builder = RegexBuilder::new(pattern);
    builder
        .case_insensitive(options.ignore_case)
        .multi_line(options.multiline)
        .dot_matches_new_line(options.singleline)
        .backtrack_limit(BACKTRACK_LIMIT);
    builder
}

/// A search stopped past its steps back.
struct Timeout;

/// How a pattern is searched.
enum Engine {
    /// By the `regex` crate, in linear time.
    Linear(regex::Regex),
    /// By `fancy-regex`, which backtracks.
    Backtracking(fancy_regex::Regex),
}

/// The byte after the character at byte `at` of `input`; past its end at its end.
fn next_char(input: &str, at: usize) -> usize {
    input[at..]
        .chars()
        .next()
        .map_or(input.len() + 1, |c| at + c.len_utf8())
}

/// The groups in .NET's order, as (number in the engine, name): the unnamed ones first,
/// then the named ones.
fn dotnet_order(names: &[Option<String>]) -> Vec<(usize, Option<String>)> {
    let numbered = names.iter().enumerate().skip(1);
    numbered
        .clone()
        .filter(|(_, name)| name.is_none())
        .chain(numbered.filter(|(_, name)| name.is_some()))
        .map(|(index, name)| (index, name.clone()))
        .collect()
}

/// Counts a text's UTF-16 units up to a byte, forward from the last place counted.
#[derive(Default)]
struct Utf16Counter {
    byte: usize,
    units: usize,
}

impl Utf16Counter {
    /// The UTF-16 place of byte `byte` of `input`.
    fn at(&mut self, input: &str, byte: usize) -> usize {
        let units = if byte >= self.byte {
            self.units + utf16_len(&input[self.byte..byte])
        } else {
            utf16_len(&input[..byte])
        };
        self.byte = byte;
        self.units = units;
        units
    }
}

/// The match `whole` of `input`, with its `groups` in .NET's `order`.
fn found_match(
    input: &str,
    whole: &Range<usize>,
    groups: &[Option<Range<usize>>],
    order: &[(usize, Option<String>)],
    counter: &mut Utf16Counter,
) -> RegexMatch {
    let index = counter.at(input, whole.start);
    let groups = order
        .iter()
        .enumerate()
        .map(|(position, (engine_index, name))| {
            let number = position + 1;
            let group = groups.get(*engine_index).cloned().flatten();
            RegexGroup {
                number,
                name: name.clone().unwrap_or_else(|| number.to_string()),
                start: group
                    .as_ref()
                    .map(|group| index + utf16_len(&input[whole.start..group.start])),
                length: group
                    .as_ref()
                    .map_or(0, |group| utf16_len(&input[group.clone()])),
                value: group.map_or_else(String::new, |group| input[group].to_owned()),
                named: name.is_some(),
            }
        })
        .collect();
    RegexMatch {
        index,
        length: utf16_len(&input[whole.clone()]),
        bytes: whole.clone(),
        value: input[whole.clone()].to_owned(),
        groups,
    }
}

/// Why `pattern` was refused: the .NET construct it uses that the engine lacks, if one,
/// else what the engine says.
fn refusal(pattern: &str, error: &Error) -> InvalidPattern {
    if matches!(error, Error::CompileError(CompileError::LookBehindNotConst)) {
        return InvalidPattern::Unsupported(DotnetConstruct::VariableLookBehind);
    }
    if has_balancing_group(pattern) {
        return InvalidPattern::Unsupported(DotnetConstruct::BalancingGroup);
    }
    let said = error.to_string();
    // The `regex` crate under it draws the pattern and points under it before its
    // message: the message alone is shown, on the status line.
    let message = said
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .map_or_else(|| said.trim().to_owned(), str::to_owned);
    InvalidPattern::Syntax(message)
}

/// Whether `pattern` opens a .NET balancing group, `(?<a-b>`, `(?<-b>`, `(?'a-b'` or
/// `(?'-b'`, outside a class and not escaped.
#[must_use]
pub fn has_balancing_group(pattern: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    let mut at = 0;
    let mut in_class = false;
    while let Some(&c) = chars.get(at) {
        if c == '\\' {
            at += 2;
            continue;
        }
        if in_class {
            in_class = c != ']';
            at += 1;
            continue;
        }
        if c == '[' {
            in_class = true;
            at += 1;
            // A `^` and a `]` at the class's start are its own.
            if chars.get(at) == Some(&'^') {
                at += 1;
            }
            if chars.get(at) == Some(&']') {
                at += 1;
            }
            continue;
        }
        if c == '('
            && chars.get(at + 1) == Some(&'?')
            && let Some(&open @ ('<' | '\'')) = chars.get(at + 2)
        {
            let close = if open == '<' { '>' } else { '\'' };
            let name: String = chars
                .iter()
                .skip(at + 3)
                .take_while(|c| **c != close && **c != ')')
                .collect();
            // `(?<=` and `(?<!` are look-behinds, not names.
            if !name.starts_with(['=', '!']) && name.contains('-') {
                return true;
            }
        }
        at += 1;
    }
    false
}

/// The length of `text` in UTF-16 units, as .NET counts it.
#[must_use]
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}
