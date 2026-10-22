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

/// The single quotes PowerShell reads as one: each is doubled to carry itself.
pub(super) const POWERSHELL_SINGLE_QUOTES: [char; 5] =
    ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// The double quotes PowerShell closes a string on besides the ASCII one.
const POWERSHELL_TYPOGRAPHIC_DOUBLE_QUOTES: [char; 3] = ['\u{201C}', '\u{201D}', '\u{201E}'];

/// How the shell reads the text at one occurrence of a placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteContext {
    /// Outside any quoted span.
    Bare,
    /// Inside a single-quoted span, where every character is literal.
    SingleQuoted,
    /// Inside a span the shell still interprets: a double-quoted string, where `$(...)` and
    /// backticks expand even between inner single quotes, a bash `$'...'` string, or right
    /// after a dollar sign (`${name}` is a variable in bash and in PowerShell).
    Expanding,
}

/// The lexer's position relative to quotes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LexerState {
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

/// The quoting context of every occurrence of `{name}` in `pattern`, in order; empty when it
/// does not appear. `powershell` selects the rules: PowerShell escapes with a backtick and
/// also closes strings on typographic quotes; bash escapes with a backslash outside single
/// quotes and has `$'...'`. Every occurrence is reported because the generator replaces
/// every occurrence.
#[must_use]
pub fn placeholder_quote_contexts(
    pattern: &str,
    name: &str,
    powershell: bool,
) -> Vec<QuoteContext> {
    if pattern.is_empty() || name.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = pattern.chars().collect();
    let placeholder: Vec<char> = format!("{{{name}}}").chars().collect();
    let escape = if powershell { '`' } else { '\\' };
    let mut contexts = Vec::new();
    let mut state = LexerState::Bare;
    let mut index = 0;
    // Where the last unescaped dollar sign outside quotes stood: a placeholder right after it
    // is `${name}`, which both shells read as a variable, so the value is not literal there
    // whatever quoting it gets.
    let mut bare_dollar: Option<usize> = None;

    while index < chars.len() {
        if chars[index..].starts_with(&placeholder) {
            contexts.push(match state {
                LexerState::Bare if index > 0 && bare_dollar == Some(index - 1) => {
                    QuoteContext::Expanding
                }
                LexerState::Bare => QuoteContext::Bare,
                LexerState::Single => QuoteContext::SingleQuoted,
                LexerState::Double | LexerState::AnsiC => QuoteContext::Expanding,
            });
            index += placeholder.len();
            continue;
        }
        let c = chars[index];
        match state {
            LexerState::Bare => {
                if c == escape {
                    index += 2;
                    continue;
                }
                if c == '$' {
                    bare_dollar = Some(index);
                }
                if !powershell && c == '$' && chars.get(index + 1) == Some(&'\'') {
                    state = LexerState::AnsiC;
                    index += 2;
                    continue;
                }
                if is_single_quote(c, powershell) {
                    state = LexerState::Single;
                } else if is_double_quote(c, powershell) {
                    state = LexerState::Double;
                }
            }
            LexerState::Single => {
                if is_single_quote(c, powershell) {
                    state = LexerState::Bare;
                }
            }
            LexerState::Double => {
                if c == escape {
                    index += 2;
                    continue;
                }
                if is_double_quote(c, powershell) {
                    state = LexerState::Bare;
                }
            }
            LexerState::AnsiC => {
                if c == '\\' {
                    index += 2;
                    continue;
                }
                if c == '\'' {
                    state = LexerState::Bare;
                }
            }
        }
        index += 1;
    }
    contexts
}
