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

//! Values put inside a script a shell reads (`sh -c "pass show {Title}"`).
//!
//! The C# strips the characters a shell reads from such a value (`SanitizeStrict`) and leaves
//! its spaces, so a title `x -c` still split the script into one more word. Here each value is
//! quoted for the shell that reads the script, or refused when the shell has no safe quoting:
//! the value is one word of the script whatever it holds.

use super::TemplateProblem;

/// The shell that reads a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dialect {
    /// `sh`, `bash`, `zsh`, and `wsl` (whose command runs on Linux).
    Posix,
    /// `cmd` and batch files.
    Cmd,
    /// Windows PowerShell and `pwsh`.
    PowerShell,
    /// A script host with no quoting to rely on (`cscript`, `mshta`...).
    Opaque,
}

/// Which quote is open at the end of the script text read so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    None,
    Single,
    Double,
}

/// The escape character of a POSIX shell.
const BACKSLASH: char = '\\';
/// The single quote of a POSIX shell.
const APOSTROPHE: char = '\'';

/// Characters `cmd` reads as syntax even inside quotes, or that cannot be escaped for it.
const CMD_SYNTAX: &str = "&|<>^%!()\"";
/// Characters that make `cmd` split a word that is not quoted.
const CMD_SEPARATORS: &str = ",;=";
/// What a double-quoted POSIX string still reads.
const POSIX_DOUBLE_ACTIVE: &str = "\"$`\\!";
/// What a double-quoted PowerShell string still reads, typographic double quotes included.
const POWERSHELL_DOUBLE_ACTIVE: &str = "\"`$\u{201c}\u{201d}\u{201e}";
/// PowerShell reads these as a single quote, like the apostrophe.
const POWERSHELL_SINGLE: &str = "'\u{2018}\u{2019}\u{201a}\u{201b}";

/// The dialect of `name` when it is a shell, as the C# `InputValidator.IsShellTarget` names
/// them; `None` for any other program.
pub(super) fn dialect_of(name: &str) -> Option<Dialect> {
    let file = name
        .trim()
        .trim_end_matches(['.', ' '])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (stem, extension) = file.rsplit_once('.').unwrap_or((&file, ""));
    match extension {
        "bat" | "cmd" => return Some(Dialect::Cmd),
        "ps1" => return Some(Dialect::PowerShell),
        "vbs" | "js" | "jse" | "vbe" | "wsf" | "hta" => return Some(Dialect::Opaque),
        _ => {}
    }
    let stem = if extension == "exe" { stem } else { &file };
    match stem {
        "cmd" => Some(Dialect::Cmd),
        "powershell" | "pwsh" => Some(Dialect::PowerShell),
        "bash" | "sh" | "zsh" | "wsl" => Some(Dialect::Posix),
        "cscript" | "wscript" | "mshta" => Some(Dialect::Opaque),
        _ => None,
    }
}

/// How the word after `previous` is read by the shell `owner`: `Some(true)` when it and every
/// word after it make up the script, `Some(false)` when only the next word is the script,
/// `None` when it is no script.
pub(super) fn script_follows(owner: Dialect, previous: &str) -> Option<bool> {
    let previous = previous.to_ascii_lowercase();
    match owner {
        Dialect::Posix => (previous == "-c").then_some(false),
        Dialect::Cmd => matches!(previous.as_str(), "/c" | "/k" | "/r").then_some(true),
        Dialect::PowerShell => {
            matches!(previous.as_str(), "-c" | "-command" | "/c" | "/command").then_some(true)
        }
        Dialect::Opaque => None,
    }
}

/// The quote left open by `text`, a script read up to a placeholder.
fn open_quote(dialect: Dialect, text: &str) -> Open {
    // The character that escapes the next one outside single quotes, if the shell has one.
    let escape = match dialect {
        Dialect::Posix => Some(BACKSLASH),
        Dialect::PowerShell => Some('`'),
        Dialect::Cmd | Dialect::Opaque => None,
    };
    let is_single = |c: char| match dialect {
        Dialect::Posix => c == APOSTROPHE,
        Dialect::PowerShell => POWERSHELL_SINGLE.contains(c),
        Dialect::Cmd | Dialect::Opaque => false,
    };
    let mut state = Open::None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if state != Open::Single && Some(c) == escape {
            chars.next();
        } else if c == '"' {
            state = match state {
                Open::Double => Open::None,
                Open::None => Open::Double,
                Open::Single => Open::Single,
            };
        } else if is_single(c) {
            state = match state {
                Open::Single => Open::None,
                Open::None => Open::Single,
                Open::Double => Open::Double,
            };
        }
    }
    state
}

/// Whether the next thing written after `text` begins a word of the script (an option could
/// stand there).
pub(super) fn begins_word(text: &str) -> bool {
    let before = text.trim_end_matches(['\'', '"']);
    before.is_empty() || before.ends_with(char::is_whitespace)
}

/// Whether `text` holds a lone `--`: the end of the options of the command it writes.
pub(super) fn has_end_of_options(text: &str) -> bool {
    text.split_whitespace().any(|word| word == "--")
}

/// `value` as one word of the script `before` (the script written so far) is turned into.
///
/// # Errors
///
/// [`TemplateProblem::UnquotableValue`] for `token` when the shell has no way to hold the value
/// as one word.
pub(super) fn quote(
    dialect: Dialect,
    before: &str,
    value: &str,
    token: &'static str,
) -> Result<String, TemplateProblem> {
    let refused = || Err(TemplateProblem::UnquotableValue(token));
    let open = open_quote(dialect, before);
    match (dialect, open) {
        (Dialect::Posix, Open::None) => Ok(format!("'{}'", value.replace('\'', r"'\''"))),
        (Dialect::Posix, Open::Single) => {
            if value.contains('\'') {
                refused()
            } else {
                Ok(value.to_owned())
            }
        }
        (Dialect::Posix, Open::Double) => {
            if value.chars().any(|c| POSIX_DOUBLE_ACTIVE.contains(c)) {
                refused()
            } else {
                Ok(value.to_owned())
            }
        }
        (Dialect::PowerShell, Open::None) => {
            let mut quoted = String::with_capacity(value.len() + 2);
            quoted.push('\'');
            for c in value.chars() {
                quoted.push(c);
                if POWERSHELL_SINGLE.contains(c) {
                    quoted.push(c);
                }
            }
            quoted.push('\'');
            Ok(quoted)
        }
        (Dialect::PowerShell, Open::Single) => {
            if value.chars().any(|c| POWERSHELL_SINGLE.contains(c)) {
                refused()
            } else {
                Ok(value.to_owned())
            }
        }
        (Dialect::PowerShell, Open::Double) => {
            if value.chars().any(|c| POWERSHELL_DOUBLE_ACTIVE.contains(c)) {
                refused()
            } else {
                Ok(value.to_owned())
            }
        }
        (Dialect::Cmd, open) => {
            if value.chars().any(|c| CMD_SYNTAX.contains(c)) {
                return refused();
            }
            if open == Open::Double {
                // A trailing backslash would escape the closing quote of the template.
                return if value.ends_with('\\') {
                    refused()
                } else {
                    Ok(value.to_owned())
                };
            }
            let splits =
                |c: char| c.is_whitespace() || CMD_SEPARATORS.contains(c) || c.is_control();
            if value.is_empty() || value.chars().any(splits) {
                if value.ends_with('\\') {
                    return refused();
                }
                Ok(format!("\"{value}\""))
            } else {
                Ok(value.to_owned())
            }
        }
        (Dialect::Opaque, _) => {
            if value
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '"' | '\''))
            {
                refused()
            } else {
                Ok(value.to_owned())
            }
        }
    }
}
