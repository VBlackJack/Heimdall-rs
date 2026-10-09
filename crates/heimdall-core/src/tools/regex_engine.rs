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
//! The engine is the `regex` crate's, not .NET's. Its syntax is .NET's for what both have:
//! classes, quantifiers lazy or not, anchors, `\b`, `\d`, `\w`, `\s` (Unicode, as .NET's),
//! groups numbered or named `(?<name>...)`, inline flags. What .NET has and it has not is
//! refused, as .NET refuses a pattern it cannot read, and named so the user sees why:
//! look-ahead and look-behind, backreferences (`\1`, `\k<name>`), atomic groups `(?>...)`
//! and conditionals `(?(...)...)`. It runs in linear time, so no pattern can hang it: the
//! C#'s one-second timeout has nothing to stop.
//!
//! Groups are numbered as .NET numbers them: the unnamed ones first, in order, then the
//! named ones.

use regex::RegexBuilder;

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
    /// `(?=...)`, `(?!...)`, `(?<=...)` or `(?<!...)`.
    LookAround,
    /// `\1` to `\9`, or `\k<name>`.
    Backreference,
    /// `(?>...)`.
    AtomicGroup,
    /// `(?(...)...)`.
    Conditional,
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
    /// The matches, in order.
    Success(Vec<RegexMatch>),
}

/// `pattern` tried on `input` with `options`, as the C# `RegexEngine.Test`
/// (`RegexEngine.cs:53-102`).
#[must_use]
pub fn test(pattern: &str, input: &str, options: RegexOptions) -> RegexTest {
    if pattern.is_empty() {
        return RegexTest::EmptyPattern;
    }
    let regex = match RegexBuilder::new(pattern)
        .case_insensitive(options.ignore_case)
        .multi_line(options.multiline)
        .dot_matches_new_line(options.singleline)
        .build()
    {
        Ok(regex) => regex,
        Err(error) => return RegexTest::InvalidPattern(refusal(pattern, &error)),
    };
    // The groups in .NET's order: (index in the engine, name).
    let names: Vec<Option<&str>> = regex.capture_names().collect();
    let order: Vec<(usize, Option<&str>)> = names
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, name)| name.is_none())
        .chain(
            names
                .iter()
                .enumerate()
                .skip(1)
                .filter(|(_, name)| name.is_some()),
        )
        .map(|(index, name)| (index, *name))
        .collect();
    let mut matches = Vec::new();
    // The UTF-16 place of a byte of the input, counted forward as matches come in order.
    let mut counted = (0_usize, 0_usize);
    let mut utf16_at = |byte: usize| {
        let (from_byte, from_utf16) = counted;
        let at = if byte >= from_byte {
            from_utf16 + utf16_len(&input[from_byte..byte])
        } else {
            utf16_len(&input[..byte])
        };
        counted = (byte, at);
        at
    };
    for captures in regex.captures_iter(input) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        let index = utf16_at(whole.start());
        let groups = order
            .iter()
            .enumerate()
            .map(|(position, (engine_index, name))| {
                let number = position + 1;
                let group = captures.get(*engine_index);
                RegexGroup {
                    number,
                    name: name.map_or_else(|| number.to_string(), str::to_owned),
                    start: group
                        .map(|group| index + utf16_len(&input[whole.start()..group.start()])),
                    length: group.map_or(0, |group| utf16_len(group.as_str())),
                    value: group.map_or_else(String::new, |group| group.as_str().to_owned()),
                    named: name.is_some(),
                }
            })
            .collect();
        matches.push(RegexMatch {
            index,
            length: utf16_len(whole.as_str()),
            bytes: whole.range(),
            value: whole.as_str().to_owned(),
            groups,
        });
    }
    RegexTest::Success(matches)
}

/// Why `pattern` was refused: the .NET construct it uses, if one, else what the engine says.
fn refusal(pattern: &str, error: &regex::Error) -> InvalidPattern {
    if let Some(construct) = dotnet_construct(pattern) {
        return InvalidPattern::Unsupported(construct);
    }
    let said = error.to_string();
    // The engine draws the pattern and points under it before its message: the message
    // alone is shown, on the status line.
    let message = said
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .map_or_else(|| said.trim().to_owned(), str::to_owned);
    InvalidPattern::Syntax(message)
}

/// The first construct of .NET's in `pattern` the engine does not have, outside a class and
/// not escaped.
#[must_use]
pub fn dotnet_construct(pattern: &str) -> Option<DotnetConstruct> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut at = 0;
    let mut in_class = false;
    while let Some(&c) = chars.get(at) {
        if c == '\\' {
            let next = chars.get(at + 1).copied();
            if !in_class {
                if next.is_some_and(|next| ('1'..='9').contains(&next)) {
                    return Some(DotnetConstruct::Backreference);
                }
                if next == Some('k') && matches!(chars.get(at + 2), Some('<' | '\'' | '{')) {
                    return Some(DotnetConstruct::Backreference);
                }
            }
            at += 2;
            continue;
        }
        if in_class {
            in_class = c != ']';
            at += 1;
            continue;
        }
        match c {
            '[' => {
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
            '(' if chars.get(at + 1) == Some(&'?') => {
                let rest: String = chars.iter().skip(at + 2).take(2).collect();
                if rest.starts_with('=')
                    || rest.starts_with('!')
                    || rest.starts_with("<=")
                    || rest.starts_with("<!")
                {
                    return Some(DotnetConstruct::LookAround);
                }
                if rest.starts_with('>') {
                    return Some(DotnetConstruct::AtomicGroup);
                }
                if rest.starts_with('(') {
                    return Some(DotnetConstruct::Conditional);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// The length of `text` in UTF-16 units, as .NET counts it.
#[must_use]
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}
