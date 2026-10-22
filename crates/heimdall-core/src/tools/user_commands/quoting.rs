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

//! Where a `{name}` placeholder sits inside a command pattern, as the shell reads it.
//!
//! A port of the C# `CommandPatternQuoting.GetPlaceholderQuoteContexts`
//! (`TwinShell.Core/Helpers/CommandPatternQuoting.cs`): a small lexer rather than a count of
//! single quotes, because the count is wrong exactly where it matters. In
//! `-Filter "Name -like '*{x}*'"` the placeholder follows one single quote, yet the shell
//! reads it inside a double-quoted string and expands `$(...)` there.
//!
//! The lexer models four spans (bare, single-quoted, double-quoted, bash `$'...'`). Anything
//! it cannot follow with certainty is reported as [`QuoteContext::Uncertain`], and the
//! generator then lets only a value of plain characters through, put in bare:
//!
//! - a command substitution (`$(...)`, a bash backtick) or a parameter expansion (`${...}`)
//!   anywhere before the placeholder: inside them the quotes open and close strings of their
//!   own, a backtick in a value ends a legacy substitution whatever single quotes surround
//!   it, and a parameter default inside double quotes reads its apostrophes as text. The
//!   lexer does not follow the nesting, it stops trusting what comes after;
//! - a placeholder right after an escape character: the generator still replaces it, so the
//!   quote it adds would be the one the escape protects;
//! - a comment or a document (`#`, PowerShell `<#` and `@'`, bash `<<`) in the pattern: a quote
//!   inside one is text;
//! - a pattern holding a line break or another control character (a script): comments and
//!   here-documents there change what a quote means.

/// The single quotes PowerShell reads as one: each is doubled to carry itself.
pub(super) const POWERSHELL_SINGLE_QUOTES: [char; 5] =
    ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// The double quotes PowerShell closes a string on besides the ASCII one.
pub(super) const POWERSHELL_TYPOGRAPHIC_DOUBLE_QUOTES: [char; 3] =
    ['\u{201C}', '\u{201D}', '\u{201E}'];

/// How the shell reads the text at one occurrence of a placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteContext {
    /// Outside any quoted span.
    Bare,
    /// Inside a single-quoted span, where every character is literal.
    SingleQuoted,
    /// Inside a double-quoted string, where `$(...)` and backticks expand even between inner
    /// single quotes, and a double quote ends the string.
    DoubleQuoted,
    /// Inside a bash `$'...'` string, which reads backslash escapes.
    AnsiC,
    /// Right after a dollar sign outside quotes: `${name}` is a variable in bash and in
    /// PowerShell.
    Expanding,
    /// A place the lexer does not follow with certainty (see the module documentation).
    Uncertain,
}

/// The lexer's position relative to quotes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Bare,
    Single,
    Double,
    AnsiC,
}

fn is_single_quote(c: char, powershell: bool) -> bool {
    if powershell {
        POWERSHELL_SINGLE_QUOTES.contains(&c)
    } else {
        c == '\''
    }
}

fn is_double_quote(c: char, powershell: bool) -> bool {
    c == '"' || (powershell && POWERSHELL_TYPOGRAPHIC_DOUBLE_QUOTES.contains(&c))
}

/// The escape character of the shell outside single quotes.
pub(super) const fn escape_character(powershell: bool) -> char {
    if powershell { '`' } else { '\\' }
}

/// Whether the text at `chars[index]` opens something the lexer does not follow: `$(`, `${`
/// that is not a placeholder written after a dollar sign, PowerShell's `@(`, or a bash
/// backtick. `placeholder` is the text being looked for.
fn opens_nested_syntax(
    chars: &[char],
    index: usize,
    placeholder: &[char],
    powershell: bool,
) -> bool {
    match chars[index] {
        '$' => match chars.get(index + 1) {
            Some('(') => true,
            Some('{') => !chars[index + 1..].starts_with(placeholder),
            _ => false,
        },
        '@' => powershell && chars.get(index + 1) == Some(&'('),
        '`' => !powershell,
        _ => false,
    }
}

/// Whether the unquoted text at `chars[index]` starts a comment (`#`, PowerShell's `<#`), a
/// here-document (bash `<<`) or a here-string (PowerShell `@'` and `@"`): a quote inside one
/// of those is text, not a quote.
fn starts_comment_or_document(chars: &[char], index: usize, powershell: bool) -> bool {
    let next = chars.get(index + 1);
    match chars[index] {
        '#' => true,
        '<' => next == Some(if powershell { &'#' } else { &'<' }),
        '@' => {
            powershell
                && next.is_some_and(|c| is_single_quote(*c, true) || is_double_quote(*c, true))
        }
        _ => false,
    }
}

/// The quoting context of every occurrence of `{name}` in `pattern`, in order; empty when it
/// does not appear. `powershell` selects the rules: PowerShell escapes with a backtick and
/// also closes strings on typographic quotes; bash escapes with a backslash outside single
/// quotes and has `$'...'`. Every occurrence is reported because the generator replaces
/// every occurrence, an escaped one included.
#[must_use]
pub fn placeholder_quote_contexts(
    pattern: &str,
    name: &str,
    powershell: bool,
) -> Vec<QuoteContext> {
    let chars: Vec<char> = pattern.chars().collect();
    let placeholder: Vec<char> = format!("{{{name}}}").chars().collect();
    let escape = escape_character(powershell);
    let mut contexts = Vec::new();
    let mut state = State::Bare;
    let mut index = 0;
    // Where the last unescaped dollar sign outside quotes stood: a placeholder right after it
    // is `${name}`, which both shells read as a variable, so the value is not literal there
    // whatever quoting it gets.
    let mut bare_dollar: Option<usize> = None;
    // Set once the lexer has met syntax it does not follow: what comes after is not trusted.
    // A script (a pattern with a line break) is not followed at all.
    let mut lost = pattern.chars().any(char::is_control);

    while index < chars.len() {
        if chars[index..].starts_with(&placeholder) {
            contexts.push(match state {
                _ if lost => QuoteContext::Uncertain,
                State::Bare if index > 0 && bare_dollar == Some(index - 1) => {
                    QuoteContext::Expanding
                }
                State::Bare => QuoteContext::Bare,
                State::Single => QuoteContext::SingleQuoted,
                State::Double => QuoteContext::DoubleQuoted,
                State::AnsiC => QuoteContext::AnsiC,
            });
            index += placeholder.len();
            continue;
        }
        let c = chars[index];
        // An escape character right before a placeholder: the generator still replaces the
        // placeholder, so what follows the escape is the value, not the brace it skips.
        if state != State::Single && c == escape && chars[index + 1..].starts_with(&placeholder) {
            contexts.push(QuoteContext::Uncertain);
            index += 1 + placeholder.len();
            continue;
        }
        match state {
            State::Single => {
                if is_single_quote(c, powershell) {
                    state = State::Bare;
                }
            }
            State::AnsiC => {
                if c == '\\' {
                    index += 2;
                    continue;
                }
                if c == '\'' {
                    state = State::Bare;
                }
            }
            State::Double | State::Bare => {
                if c == escape {
                    index += 2;
                    continue;
                }
                if opens_nested_syntax(&chars, index, &placeholder, powershell)
                    || (state == State::Bare
                        && starts_comment_or_document(&chars, index, powershell))
                {
                    lost = true;
                }
                if state == State::Double {
                    if is_double_quote(c, powershell) {
                        state = State::Bare;
                    }
                } else {
                    if c == '$' {
                        bare_dollar = Some(index);
                    }
                    if !powershell && c == '$' && chars.get(index + 1) == Some(&'\'') {
                        state = State::AnsiC;
                        index += 2;
                        continue;
                    }
                    if is_single_quote(c, powershell) {
                        state = State::Single;
                    } else if is_double_quote(c, powershell) {
                        state = State::Double;
                    }
                }
            }
        }
        index += 1;
    }
    contexts
}

/// The contexts of `{name}` in `pattern` with one entry per occurrence, none missing: should
/// the lexer ever report fewer than the pattern holds, the unreported ones count as
/// [`QuoteContext::Uncertain`].
#[must_use]
pub fn checked_placeholder_contexts(
    pattern: &str,
    name: &str,
    powershell: bool,
) -> Vec<QuoteContext> {
    let mut contexts = placeholder_quote_contexts(pattern, name, powershell);
    let written = pattern.matches(&format!("{{{name}}}")).count();
    while contexts.len() < written {
        contexts.push(QuoteContext::Uncertain);
    }
    contexts
}
