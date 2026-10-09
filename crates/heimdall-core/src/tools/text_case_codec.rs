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

//! A text's case changed as the C# `TextCaseCodec` (`Heimdall.Core/Codecs/TextCaseCodec.cs`)
//! changes it. Upper and lower case change every letter; the other styles split the text
//! into words first, as the C# `SplitWords`: at a lower-case ASCII letter followed by an
//! upper-case one (`helloWorld`), before the last capital of a run followed by a lower-case
//! letter (`XMLParser`), and at every run of spaces, `_` and `-`.
//!
//! Letters change case one by one, as .NET's invariant culture changes them: a letter whose
//! case is more than one letter (`ß` in upper case) stays as it is.

/// The case styles, as the C# `TextCaseStyle` and in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextCaseStyle {
    /// `helloWorld`.
    Camel,
    /// `HelloWorld`.
    Pascal,
    /// `hello_world`.
    Snake,
    /// `hello-world`.
    Kebab,
    /// `HELLO WORLD`: every letter, nothing split.
    Upper,
    /// `hello world`: every letter, nothing split.
    Lower,
    /// `Hello World`.
    Title,
    /// `HELLO_WORLD`.
    Constant,
}

impl TextCaseStyle {
    /// Every style, in the order the C# tool's buttons show them.
    pub const ALL: [Self; 8] = [
        Self::Camel,
        Self::Pascal,
        Self::Snake,
        Self::Kebab,
        Self::Upper,
        Self::Lower,
        Self::Title,
        Self::Constant,
    ];
}

/// What joins the words of snake and constant case.
const SNAKE_SEPARATOR: &str = "_";

/// What joins the words of kebab case.
const KEBAB_SEPARATOR: &str = "-";

/// What joins the words of title case.
const TITLE_SEPARATOR: &str = " ";

/// `input` in `style`, as the C# `TextCaseCodec.Convert` (`TextCaseCodec.cs:37-53`).
#[must_use]
pub fn convert(input: &str, style: TextCaseStyle) -> String {
    match style {
        TextCaseStyle::Camel => {
            let words = split_words(input);
            let mut out = String::with_capacity(input.len());
            for (index, word) in words.iter().enumerate() {
                if index == 0 {
                    out.push_str(&lower(word));
                } else {
                    out.push_str(&capitalize(word));
                }
            }
            out
        }
        TextCaseStyle::Pascal => split_words(input)
            .iter()
            .map(|word| capitalize(word))
            .collect(),
        TextCaseStyle::Snake => join(input, lower, SNAKE_SEPARATOR),
        TextCaseStyle::Kebab => join(input, lower, KEBAB_SEPARATOR),
        TextCaseStyle::Upper => upper(input),
        TextCaseStyle::Lower => lower(input),
        TextCaseStyle::Title => join(input, capitalize, TITLE_SEPARATOR),
        TextCaseStyle::Constant => join(input, upper, SNAKE_SEPARATOR),
    }
}

/// The words of `input`, each changed by `change`, joined by `separator`.
fn join(input: &str, change: fn(&str) -> String, separator: &str) -> String {
    split_words(input)
        .iter()
        .map(|word| change(word))
        .collect::<Vec<String>>()
        .join(separator)
}

/// The words of `input`, as the C# `SplitWords` (`TextCaseCodec.cs:55-67`).
fn split_words(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    for (index, &c) in chars.iter().enumerate() {
        if is_separator(c) {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            continue;
        }
        let previous = index.checked_sub(1).and_then(|before| chars.get(before));
        let next = chars.get(index + 1);
        // `(?<=[a-z])(?=[A-Z])`, then `(?<=[A-Z])(?=[A-Z][a-z])`: ASCII letters only, as the
        // C#'s classes.
        let camel = previous.is_some_and(char::is_ascii_lowercase) && c.is_ascii_uppercase();
        let acronym = previous.is_some_and(char::is_ascii_uppercase)
            && c.is_ascii_uppercase()
            && next.is_some_and(char::is_ascii_lowercase);
        if (camel || acronym) && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        word.push(c);
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// Whether `c` splits words, as the C#'s `[\s_\-]`.
fn is_separator(c: char) -> bool {
    c.is_whitespace() || c == '_' || c == '-'
}

/// `word` with its first letter in upper case and the rest in lower case, as the C#
/// `Capitalize`.
fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::with_capacity(word.len());
    out.push(upper_char(first));
    out.push_str(&lower(chars.as_str()));
    out
}

/// `text` in upper case, as .NET's `ToUpperInvariant`.
fn upper(text: &str) -> String {
    text.chars().map(upper_char).collect()
}

/// `text` in lower case, as .NET's `ToLowerInvariant`.
fn lower(text: &str) -> String {
    text.chars().map(lower_char).collect()
}

/// `c` in upper case when that is one character, else `c`.
fn upper_char(c: char) -> char {
    single(c.to_uppercase()).unwrap_or(c)
}

/// `c` in lower case when that is one character, else `c`.
fn lower_char(c: char) -> char {
    single(c.to_lowercase()).unwrap_or(c)
}

/// The only character of `chars`, if it has one only.
fn single(mut chars: impl Iterator<Item = char>) -> Option<char> {
    let first = chars.next()?;
    chars.next().is_none().then_some(first)
}
