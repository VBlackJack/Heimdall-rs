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

use std::path::PathBuf;

use heimdall_i18n::{
    CheckError, Gap, GapKind, find_gaps, fixed_one_variants, message_ids, message_variables,
};

const DOMAIN: &str = "demo";

fn fixture(case: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(case)
}

fn gap(language: &str, key: &str, kind: GapKind) -> Gap {
    Gap {
        language: language.to_owned(),
        key: key.to_owned(),
        kind,
    }
}

#[test]
fn complete_languages_have_no_gap() {
    let gaps = find_gaps(&fixture("complete"), DOMAIN).expect("fixture is valid");
    assert!(gaps.is_empty(), "unexpected gaps: {gaps:?}");
}

#[test]
fn a_key_absent_from_one_language_is_reported_missing() {
    let gaps = find_gaps(&fixture("missing"), DOMAIN).expect("fixture is valid");
    assert_eq!(gaps, vec![gap("fr", "demo-farewell", GapKind::Missing)]);
}

#[test]
fn a_key_absent_from_the_fallback_is_reported_extra() {
    let gaps = find_gaps(&fixture("extra"), DOMAIN).expect("fixture is valid");
    assert_eq!(gaps, vec![gap("es", "demo-leftover", GapKind::Extra)]);
}

#[test]
fn a_file_that_does_not_parse_is_an_error_not_a_pass() {
    let error = find_gaps(&fixture("broken"), DOMAIN).expect_err("fr does not parse");
    assert!(matches!(error, CheckError::Parse { .. }), "got {error:?}");
}

#[test]
fn a_missing_language_file_is_an_error_not_a_pass() {
    let error = find_gaps(&fixture("absent"), DOMAIN).expect_err("es file is absent");
    assert!(matches!(error, CheckError::Io { .. }), "got {error:?}");
}

#[test]
fn terms_and_comments_are_not_counted_as_messages() {
    let source = "# A comment\n-brand = Heimdall\nshell-title = { -brand }\n";
    let ids = message_ids(source).expect("valid Fluent");
    assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec!["shell-title"]);
}

#[test]
fn a_translation_naming_other_variables_is_reported() {
    let gaps = find_gaps(&fixture("variables"), DOMAIN).expect("fixture is valid");
    assert_eq!(
        gaps,
        vec![
            gap("fr", "demo-name", GapKind::Variables),
            gap("es", "demo-count", GapKind::Variables),
        ],
        "a renamed variable, and one left out of every variant"
    );
}

#[test]
fn variables_are_found_in_selectors_calls_variants_and_attributes() {
    let source = "demo = { NUMBER($size, $style) ->\n    [one] { $unit }\n   *[other] { { $nested } }\n}\n    .title = { $hint }\n";
    let variables = message_variables(source).expect("valid Fluent");
    assert_eq!(
        variables["demo"]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["hint", "nested", "size", "style", "unit"]
    );
}

#[test]
fn a_one_variant_must_name_its_number() {
    let source = concat!(
        "fixed = { $count ->\n    [one] 1 file\n   *[other] { $count } files\n}\n",
        "named = { $count ->\n    [one] { $count } file\n   *[other] { $count } files\n}\n",
        "other-key = { $count ->\n    [few] a few\n   *[other] { $count } files\n}\n",
        "nested = { $a ->\n   *[other] { $b ->\n        [one] 1 thing\n       *[other] { $b } things\n    }\n}\n",
    );
    assert_eq!(
        fixed_one_variants(source).expect("valid Fluent"),
        ["fixed", "nested"]
    );
}
