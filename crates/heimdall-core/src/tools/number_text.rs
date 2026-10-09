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

//! Numbers as the C# tools read and write them with .NET: `int.TryParse` and the "N0"
//! and "F1" formats, the culture's separators given by the caller.

/// Digits in a group of "N0".
const GROUP_DIGITS: usize = 3;

/// `text` as .NET's `int.TryParse` with `NumberStyles.Integer` reads it: spaces around, an
/// optional sign, then digits, within a 32-bit integer; `None` otherwise.
#[must_use]
pub fn parse_int32(text: &str) -> Option<i32> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix(['-', '+']).unwrap_or(trimmed);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    trimmed.parse().ok()
}

/// `value` with its digits grouped in threes by `separator`, as "N0" writes a whole
/// number in the culture whose separator it is.
#[must_use]
pub fn group_digits(value: u128, separator: &str) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() * 2);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(GROUP_DIGITS) {
            grouped.push_str(separator);
        }
        grouped.push(digit);
    }
    grouped
}

/// `value` with one decimal, as "F1" writes it, rounded from its exact binary value as
/// .NET does since .NET Core 3.0, its decimal mark `decimal_separator`.
#[must_use]
pub fn one_decimal(value: f64, decimal_separator: &str) -> String {
    format!("{value:.1}").replacen('.', decimal_separator, 1)
}
