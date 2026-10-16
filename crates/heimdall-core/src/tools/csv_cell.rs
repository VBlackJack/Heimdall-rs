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

//! Cells of the CSV and tab-separated exports, as the C# `InputValidator.SanitizeCsvCell`
//! writes them: a value a spreadsheet would read as a formula is prefixed with an apostrophe.

/// The characters a spreadsheet starts a formula with, as the C# `SanitizeCsvCell`.
const FORMULA_STARTS: [char; 4] = ['=', '+', '-', '@'];

/// The control characters the C# also refuses at the start of a cell.
const CONTROL_STARTS: [char; 3] = ['\t', '\r', '\n'];

/// `value` safe to open in a spreadsheet: an apostrophe before it when it starts with a
/// control character, or with a formula character after any leading spaces, which some
/// spreadsheets skip.
#[must_use]
pub fn sanitize(value: &str) -> String {
    let starts_control = value.starts_with(CONTROL_STARTS);
    let starts_formula = value.trim_start().starts_with(FORMULA_STARTS);
    if starts_control || starts_formula {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

/// `value` sanitized, then quoted as RFC 4180 quotes a field: in double quotes, each
/// double quote doubled.
#[must_use]
pub fn quoted(value: &str) -> String {
    format!("\"{}\"", sanitize(value).replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_formula_or_a_control_start_is_neutralised_and_nothing_else_changes() {
        assert_eq!(sanitize("=cmd|' /C calc'!A0"), "'=cmd|' /C calc'!A0");
        assert_eq!(sanitize("+1"), "'+1");
        assert_eq!(sanitize("-1"), "'-1");
        assert_eq!(sanitize("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(sanitize("  =1+1"), "'  =1+1");
        assert_eq!(sanitize("\tx"), "'\tx");
        assert_eq!(sanitize("\nx"), "'\nx");
        assert_eq!(sanitize("plain - text"), "plain - text");
        assert_eq!(sanitize(""), "");
    }

    #[test]
    fn a_quoted_cell_doubles_its_quotes_after_sanitizing() {
        assert_eq!(quoted("hello \"world\""), "\"hello \"\"world\"\"\"");
        assert_eq!(quoted("=1"), "\"'=1\"");
    }
}
