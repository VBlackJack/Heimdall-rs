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

//! The text tools' engines against the C# tests' vectors: `JsonCodecTests.cs`,
//! `RegexEngineTests.cs`, `TextCaseCodecTests.cs` and `DiffEngineTests.cs`.

use std::time::Duration;

use heimdall_core::tools::diff_engine::{
    self, DEFAULT_MAX_LINE_COUNT, DiffLineKind, DiffOptions, TextDiff, WordSegment,
};
use heimdall_core::tools::json_codec::{self, JsonFormat, MAX_INPUT_BYTES};
use heimdall_core::tools::regex_engine::{
    self, DotnetConstruct, InvalidPattern, RegexOptions, RegexTest,
};
use heimdall_core::tools::text_case_codec::{TextCaseStyle, convert};

/// The line break the tests format with, as .NET's on Windows.
const CRLF: &str = "\r\n";

fn json(input: &str, indented: bool) -> JsonFormat {
    json_codec::format(input, indented, CRLF)
}

fn formatted(input: &str, indented: bool) -> String {
    match json(input, indented) {
        JsonFormat::Success(out) => out,
        other => panic!("{input:?} not formatted: {other:?}"),
    }
}

#[test]
fn json_blank_input_is_empty() {
    for input in ["", " ", "\t\r\n"] {
        assert_eq!(json(input, true), JsonFormat::Empty);
        assert_eq!(
            json_codec::format_input(input, true, CRLF),
            JsonFormat::Empty
        );
    }
}

#[test]
fn json_prettified_and_minified_as_the_csharp() {
    let pretty = formatted("{\"a\":1,\"b\":2}", true);
    assert!(pretty.contains(CRLF), "{pretty:?}");
    assert!(pretty.contains("\"a\": 1"), "{pretty:?}");
    assert_eq!(pretty, "{\r\n  \"a\": 1,\r\n  \"b\": 2\r\n}");
    assert_eq!(
        formatted("{\n  \"a\": 1,\n  \"b\": 2\n}", false),
        "{\"a\":1,\"b\":2}"
    );
    for (input, expected) in [("true", "true"), ("123", "123"), ("\"text\"", "\"text\"")] {
        assert_eq!(formatted(input, false), expected);
    }
    let array = formatted("[1,2,3]", true);
    assert!(array.contains('[') && array.contains("  2"), "{array:?}");
    assert!(formatted("{\"a\":{\"b\":{\"c\":[1,2,{\"d\":4}]}}}", true).contains("\"d\": 4"));
    let prettified = formatted("{\"a\":1,\"b\":[1,2,3]}", true);
    assert_eq!(formatted(&prettified, false), "{\"a\":1,\"b\":[1,2,3]}");
}

#[test]
fn json_strings_and_numbers_are_written_as_dotnet_writes_them() {
    let relaxed = formatted("{\"text\":\"é<&/'\"}", false);
    for kept in ["é", "<", "&"] {
        assert!(relaxed.contains(kept), "{relaxed:?}");
    }
    assert!(!relaxed.to_lowercase().contains("\\u00e9"), "{relaxed:?}");
    let escaped = formatted("{\"text\":\"line\\t\\\"quote\\\"\\\\slash\\u1234\"}", false);
    for kept in ["\\t", "\\\"", "\\\\", "\u{1234}"] {
        assert!(escaped.contains(kept), "{escaped:?}");
    }
    assert_eq!(formatted("{\"n\":1.23e+10}", false), "{\"n\":1.23e+10}");
}

#[test]
fn json_errors_say_what_and_where() {
    let JsonFormat::ParseError(error) = json("{\"a\":", true) else {
        panic!("refused");
    };
    assert!(!error.message.is_empty());
    let JsonFormat::ParseError(error) = json("{\n  \"a\": 1,\n}", true) else {
        panic!("refused");
    };
    assert_eq!(error.line, Some(3), "{error:?}");
    assert!(error.column.is_some());
    assert!(!error.message.contains("at line"), "{error:?}");
}

#[test]
fn json_past_five_megabytes_is_refused() {
    let large = format!("\"{}\"", "a".repeat(MAX_INPUT_BYTES));
    assert_eq!(
        json_codec::format_input(&large, true, CRLF),
        JsonFormat::InputTooLarge
    );
}

fn regex(pattern: &str, input: &str, options: RegexOptions) -> RegexTest {
    regex_engine::test(pattern, input, options)
}

fn matches(pattern: &str, input: &str, options: RegexOptions) -> Vec<regex_engine::RegexMatch> {
    match regex(pattern, input, options) {
        RegexTest::Success(matches) => matches,
        other => panic!("{pattern:?}: {other:?}"),
    }
}

#[test]
fn regex_empty_and_invalid_patterns_are_said() {
    assert_eq!(
        regex("", "abc", RegexOptions::default()),
        RegexTest::EmptyPattern
    );
    let RegexTest::InvalidPattern(InvalidPattern::Syntax(message)) =
        regex("[", "abc", RegexOptions::default())
    else {
        panic!("refused");
    };
    assert!(
        !message.is_empty() && !message.contains('\n'),
        "{message:?}"
    );
    assert!(matches("a", "", RegexOptions::default()).is_empty());
    assert!(matches("z", "abc", RegexOptions::default()).is_empty());
}

#[test]
fn regex_matches_their_places_values_and_groups() {
    let found = matches("b+", "abbb c", RegexOptions::default());
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].index, found[0].length), (1, 3));
    assert_eq!(found[0].value, "bbb");
    let found = matches("\\d+", "a1 b22 c333", RegexOptions::default());
    let values: Vec<&str> = found.iter().map(|found| found.value.as_str()).collect();
    assert_eq!(values, ["1", "22", "333"]);
    let found = matches("(ab)(cd)", "xxabcdyy", RegexOptions::default());
    let numbers: Vec<usize> = found[0].groups.iter().map(|group| group.number).collect();
    assert_eq!(numbers, [1, 2]);
    let found = matches("(?<word>ab)c", "abc", RegexOptions::default());
    assert_eq!(found[0].groups.len(), 1);
    assert_eq!(found[0].groups[0].name, "word");
    assert!(found[0].groups[0].named);
    assert_eq!(found[0].groups[0].value, "ab");
    assert!(!matches("(ab)c", "abc", RegexOptions::default())[0].groups[0].named);
    let missed = &matches("(a)?b", "b", RegexOptions::default())[0].groups[0];
    assert_eq!((missed.start, missed.length), (None, 0));
    assert!(missed.value.is_empty());
    for (pattern, input, count) in [
        ("a+", "aaa", 1),
        ("\\w+", "alpha beta", 2),
        ("(?<x>ab)", "ab ab", 2),
    ] {
        assert_eq!(
            matches(pattern, input, RegexOptions::default()).len(),
            count
        );
    }
}

#[test]
fn regex_options_change_what_matches() {
    let ignore_case = RegexOptions {
        ignore_case: true,
        ..RegexOptions::default()
    };
    assert_eq!(matches("abc", "ABC", ignore_case).len(), 1);
    assert!(matches("abc", "ABC", RegexOptions::default()).is_empty());
    let multiline = RegexOptions {
        multiline: true,
        ..RegexOptions::default()
    };
    assert_eq!(matches("^abc$", "zzz\nabc\nzzz", multiline).len(), 1);
    assert!(matches("^abc$", "zzz\nabc\nzzz", RegexOptions::default()).is_empty());
    let singleline = RegexOptions {
        singleline: true,
        ..RegexOptions::default()
    };
    assert_eq!(matches("a.*c", "a\nb\nc", singleline).len(), 1);
    assert!(matches("a.*c", "a\nb\nc", RegexOptions::default()).is_empty());
}

#[test]
fn regex_places_are_counted_in_utf16_units_and_groups_numbered_as_dotnet() {
    let found = matches("é+", "caféé", RegexOptions::default());
    assert_eq!(found[0].value, "éé");
    let found = matches("x", "\u{1F600}x", RegexOptions::default());
    assert_eq!(found[0].index, 2, "an emoji is two UTF-16 units");
    // .NET numbers the unnamed groups first.
    let found = matches("(?<n>a)(b)", "ab", RegexOptions::default());
    let groups: Vec<(usize, &str)> = found[0]
        .groups
        .iter()
        .map(|group| (group.number, group.value.as_str()))
        .collect();
    assert_eq!(groups, [(1, "b"), (2, "a")]);
}

#[test]
fn regex_a_runaway_pattern_times_out_as_the_csharp() {
    // `RegexEngineTests.Test_Timeout_ReturnsMatchTimeout`: 20,000 `a`, a 1 ms timeout. The
    // C#'s `(a+)+b` needs no backtracking here and runs in linear time; made to backtrack,
    // by an atomic group before it, it times out on the clock as in the C#.
    let input = "a".repeat(20_000);
    assert_eq!(
        regex_engine::test_within(
            r"(?>a)(a+)+b",
            &input,
            RegexOptions::default(),
            Duration::from_millis(1)
        ),
        RegexTest::MatchTimeout
    );
    assert!(matches("(a+)+b", &input, RegexOptions::default()).is_empty());
    // Too many steps back stops a search whatever the clock says.
    assert_eq!(
        regex_engine::test_within(
            r"(a+)+\1b",
            &input,
            RegexOptions::default(),
            Duration::from_secs(60)
        ),
        RegexTest::MatchTimeout
    );
    assert_eq!(regex_engine::DEFAULT_TIMEOUT, Duration::from_secs(1));
}

#[test]
fn regex_dotnet_constructs_run_as_in_dotnet() {
    let values = |pattern: &str, input: &str| -> Vec<(usize, String)> {
        matches(pattern, input, RegexOptions::default())
            .into_iter()
            .map(|found| (found.index, found.value))
            .collect()
    };
    let owned = |pairs: &[(usize, &str)]| -> Vec<(usize, String)> {
        pairs
            .iter()
            .map(|(at, text)| (*at, (*text).to_owned()))
            .collect()
    };
    assert_eq!(values(r"a(?=b)", "ab ac"), owned(&[(0, "a")]));
    assert_eq!(values(r"a(?!b)", "ab ac"), owned(&[(3, "a")]));
    assert_eq!(values(r"(?<=a)b", "abcab"), owned(&[(1, "b"), (4, "b")]));
    assert_eq!(values(r"(?<!a)b", "abcb"), owned(&[(3, "b")]));
    assert_eq!(values(r"(\w)\1", "aabcc"), owned(&[(0, "aa"), (3, "cc")]));
    assert_eq!(values(r"(?<c>\w)\k<c>", "xyy"), owned(&[(1, "yy")]));
    assert_eq!(values(r"(?>a+)b", "aab"), owned(&[(0, "aab")]));
    assert!(
        values(r"(?>a+)ab", "aab").is_empty(),
        "an atomic group gives nothing back"
    );
    assert_eq!(
        values(r"(a)?(?(1)b|c)", "ab c"),
        owned(&[(0, "ab"), (3, "c")])
    );
    // Matches follow one another as .NET's: an empty one where the last one ended.
    assert_eq!(values("a*", "aab"), owned(&[(0, "aa"), (2, ""), (3, "")]));
    assert_eq!(values("(?=a)", "aba"), owned(&[(0, ""), (2, "")]));
    // Places in UTF-16 units and .NET's group numbers on the backtracking path too.
    let found = matches(r"(?<n>a)(b)(?=c)", "\u{1F600}abc", RegexOptions::default());
    assert_eq!(found[0].index, 2);
    let groups: Vec<(usize, &str)> = found[0]
        .groups
        .iter()
        .map(|group| (group.number, group.value.as_str()))
        .collect();
    assert_eq!(groups, [(1, "b"), (2, "a")]);
}

#[test]
fn regex_dotnet_constructs_the_engine_lacks_are_named() {
    for (pattern, construct) in [
        ("(?<=a+)b", DotnetConstruct::VariableLookBehind),
        ("(?<open-close>a)", DotnetConstruct::BalancingGroup),
        ("(?<-close>a)", DotnetConstruct::BalancingGroup),
    ] {
        assert_eq!(
            regex(pattern, "ab", RegexOptions::default()),
            RegexTest::InvalidPattern(InvalidPattern::Unsupported(construct)),
            "{pattern}"
        );
    }
    // Escaped or in a class, a balancing group's spelling is plain characters.
    assert!(!regex_engine::has_balancing_group(r"\(?<a-b>"));
    assert!(!regex_engine::has_balancing_group("[(?<a-b>]"));
    assert!(!regex_engine::has_balancing_group("(?<=a-b)"));
}

#[test]
fn text_case_vectors_of_the_csharp() {
    let cases: &[(TextCaseStyle, &[(&str, &str)])] = &[
        (
            TextCaseStyle::Camel,
            &[
                ("", ""),
                ("hello world", "helloWorld"),
                ("XMLParser", "xmlParser"),
                ("xml_parser", "xmlParser"),
                ("héllo wörld", "hélloWörld"),
            ],
        ),
        (
            TextCaseStyle::Pascal,
            &[
                ("", ""),
                ("hello world", "HelloWorld"),
                ("XMLParser", "XmlParser"),
                ("xml_parser", "XmlParser"),
                ("__hello-world__", "HelloWorld"),
            ],
        ),
        (
            TextCaseStyle::Snake,
            &[
                ("", ""),
                ("hello world", "hello_world"),
                ("XMLParser", "xml_parser"),
                ("xml-parser", "xml_parser"),
            ],
        ),
        (
            TextCaseStyle::Kebab,
            &[
                ("", ""),
                ("hello world", "hello-world"),
                ("XMLParser", "xml-parser"),
                ("xml_parser", "xml-parser"),
            ],
        ),
        (
            TextCaseStyle::Upper,
            &[
                ("", ""),
                ("hello world", "HELLO WORLD"),
                ("XMLParser", "XMLPARSER"),
                ("snake_case", "SNAKE_CASE"),
            ],
        ),
        (
            TextCaseStyle::Lower,
            &[
                ("", ""),
                ("Hello World", "hello world"),
                ("XMLParser", "xmlparser"),
                ("SNAKE_CASE", "snake_case"),
            ],
        ),
        (
            TextCaseStyle::Title,
            &[
                ("", ""),
                ("hello world", "Hello World"),
                ("XMLParser", "Xml Parser"),
                ("xml_parser", "Xml Parser"),
                ("helloWorld_test-case", "Hello World Test Case"),
            ],
        ),
        (
            TextCaseStyle::Constant,
            &[
                ("", ""),
                ("hello world", "HELLO_WORLD"),
                ("XMLParser", "XML_PARSER"),
                ("xml-parser", "XML_PARSER"),
            ],
        ),
    ];
    for (style, vectors) in cases {
        for (input, expected) in *vectors {
            assert_eq!(convert(input, *style), *expected, "{style:?} of {input:?}");
        }
    }
    assert_eq!(convert("straße", TextCaseStyle::Upper), "STRAßE", "as .NET");
}

fn diff(original: &str, modified: &str, options: DiffOptions) -> diff_engine::DiffLines {
    match diff_engine::diff(original, modified, options) {
        TextDiff::Success(lines) => lines,
        TextDiff::InputTooLarge => panic!("too large"),
    }
}

fn kinds(lines: &diff_engine::DiffLines) -> Vec<DiffLineKind> {
    lines.lines.iter().map(|line| line.kind).collect()
}

#[test]
fn diff_counts_and_kinds_of_the_csharp() {
    let none = DiffOptions::default();
    let both_empty = diff("", "", none);
    assert!(both_empty.lines.is_empty());
    assert_eq!(
        (both_empty.added, both_empty.removed, both_empty.unchanged),
        (0, 0, 0)
    );
    let same = diff("a\nb", "a\nb", none);
    assert_eq!(same.unchanged, 2);
    assert_eq!(kinds(&same), [DiffLineKind::Unchanged; 2]);
    let added = diff("", "a\nb", none);
    assert_eq!((added.added, added.removed), (2, 0));
    assert_eq!(kinds(&added), [DiffLineKind::Added; 2]);
    let removed = diff("a\nb", "", none);
    assert_eq!((removed.added, removed.removed), (0, 2));
    let middle = diff("a\nb\nc", "a\nx\nc", none);
    assert_eq!((middle.added, middle.removed, middle.unchanged), (1, 1, 2));
    assert_eq!(
        kinds(&middle),
        [
            DiffLineKind::Unchanged,
            DiffLineKind::Removed,
            DiffLineKind::Added,
            DiffLineKind::Unchanged
        ]
    );
    assert_eq!(
        kinds(&diff("a", "b", none)),
        [DiffLineKind::Removed, DiffLineKind::Added]
    );
    let mixed = diff("a\nb\nc", "a\nc\nd", none);
    for (kind, count) in [
        (DiffLineKind::Added, mixed.added),
        (DiffLineKind::Removed, mixed.removed),
        (DiffLineKind::Unchanged, mixed.unchanged),
    ] {
        assert_eq!(
            mixed.lines.iter().filter(|line| line.kind == kind).count(),
            count
        );
    }
}

#[test]
fn diff_line_breaks_and_normalization_as_the_csharp() {
    assert_eq!(
        diff("a\r\nb\r\n", "a\nb\n", DiffOptions::default()).unchanged,
        3
    );
    let trailing = diff("a\n", "a", DiffOptions::default());
    let last = trailing.lines.last().expect("a line");
    assert_eq!((last.kind, last.text.as_str()), (DiffLineKind::Removed, ""));
    for (original, modified, ignore_whitespace, ignore_case) in [
        ("  hello\tworld  ", "hello world", true, false),
        ("HELLO", "hello", false, true),
        ("  HELLO\tWORLD ", "hello world", true, true),
    ] {
        let lines = diff(
            original,
            modified,
            DiffOptions {
                ignore_whitespace,
                ignore_case,
                max_line_count: None,
            },
        );
        assert_eq!(kinds(&lines), [DiffLineKind::Unchanged]);
        assert_eq!(
            lines.lines[0].text, original,
            "the original's text is shown"
        );
    }
}

#[test]
fn diff_past_the_line_limit_is_refused() {
    let over = vec!["x"; DEFAULT_MAX_LINE_COUNT + 1].join("\n");
    assert_eq!(
        diff_engine::diff(&over, "", DiffOptions::default()),
        TextDiff::InputTooLarge
    );
    assert_eq!(
        diff_engine::diff("", &over, DiffOptions::default()),
        TextDiff::InputTooLarge
    );
    let at = vec!["x"; DEFAULT_MAX_LINE_COUNT].join("\n");
    assert_eq!(
        diff(&at, &at, DiffOptions::default()).unchanged,
        DEFAULT_MAX_LINE_COUNT
    );
    assert_eq!(
        diff_engine::diff(
            "a\nb",
            "a\nb",
            DiffOptions {
                max_line_count: Some(1),
                ..DiffOptions::default()
            }
        ),
        TextDiff::InputTooLarge
    );
}

fn segments(pairs: &[(&str, bool)]) -> Vec<WordSegment> {
    pairs
        .iter()
        .map(|(text, changed)| WordSegment::new(text, *changed))
        .collect()
}

#[test]
fn word_diff_vectors_of_the_csharp() {
    let same = diff_engine::word_diff("alpha beta", "alpha beta");
    assert!(
        same.old
            .iter()
            .chain(&same.new)
            .all(|segment| !segment.changed)
    );
    let different = diff_engine::word_diff("alpha beta", "gamma delta");
    assert_eq!(
        different.old,
        segments(&[("alpha", true), (" ", false), ("beta", true)])
    );
    assert_eq!(
        different.new,
        segments(&[("gamma", true), (" ", false), ("delta", true)])
    );
    let middle = diff_engine::word_diff("alpha beta gamma", "alpha delta gamma");
    assert_eq!(
        middle.old,
        segments(&[("alpha ", false), ("beta", true), (" gamma", false)])
    );
    assert_eq!(
        middle.new,
        segments(&[("alpha ", false), ("delta", true), (" gamma", false)])
    );
    assert!(
        diff_engine::word_diff("foo  bar", "foo baz")
            .old
            .iter()
            .any(|segment| segment.text.contains("  "))
    );
    let empty = diff_engine::word_diff("", "");
    assert!(empty.old.is_empty() && empty.new.is_empty());
    let new_only = diff_engine::word_diff("", "alpha beta");
    assert!(new_only.old.is_empty());
    assert_eq!(new_only.new, segments(&[("alpha beta", true)]));
    let old_only = diff_engine::word_diff("alpha beta", "");
    assert_eq!(old_only.old, segments(&[("alpha beta", true)]));
    assert!(old_only.new.is_empty());
    assert_eq!(
        diff_engine::word_diff("alpha beta gamma", "omega theta gamma").old,
        segments(&[
            ("alpha", true),
            (" ", false),
            ("beta", true),
            (" gamma", false)
        ])
    );
    let mixed = diff_engine::word_diff("one two three four", "one three four five");
    assert_eq!(
        mixed.old,
        segments(&[("one", false), (" two", true), (" three four", false)])
    );
    assert_eq!(
        mixed.new,
        segments(&[("one three four", false), (" five", true)])
    );
}
