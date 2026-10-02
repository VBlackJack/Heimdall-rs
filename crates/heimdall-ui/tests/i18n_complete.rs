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

use std::path::Path;

use heimdall_i18n::GapKind;

/// New text is written in English first and translated later, in batches: a key missing
/// from another language is a translation still to do, shown in English meanwhile. A key
/// English does not have is refused: it is dead, or a typo that would never be shown. So is
/// a translation naming other variables than the English: it would show an error in place
/// of a value, or leave the value out.
#[test]
fn no_language_holds_a_key_english_lacks_or_other_variables() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let gaps = heimdall_i18n::find_gaps(crate_root, env!("CARGO_PKG_NAME"))
        .expect("every language file exists and parses");
    let refused: Vec<_> = gaps
        .iter()
        .filter(|gap| gap.kind != GapKind::Missing)
        .collect();
    assert!(refused.is_empty(), "refused: {refused:#?}");
    let missing = gaps.len() - refused.len();
    if missing > 0 {
        eprintln!("translations still to do: {missing}");
    }
}

/// French puts 0 in the `one` plural: a variant reading "1 session" would say it of none. So
/// every `one` variant, in every language, names its number.
#[test]
fn every_one_variant_names_its_number() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for language in heimdall_i18n::SUPPORTED_LANGUAGES {
        let path = heimdall_i18n::ftl_path(crate_root, language, env!("CARGO_PKG_NAME"));
        let source = std::fs::read_to_string(&path).expect("readable");
        let fixed = heimdall_i18n::fixed_one_variants(&source).expect("parses");
        assert!(fixed.is_empty(), "{language}: {fixed:?}");
    }
}

/// A count is worded by the plural rule of its language, as the C# words it: "(s)" reads as a
/// form left unfinished, and in French it is wrong for 0 and 1 alike.
#[test]
fn no_text_leaves_a_plural_to_the_reader() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for language in heimdall_i18n::SUPPORTED_LANGUAGES {
        let path = heimdall_i18n::ftl_path(crate_root, language, env!("CARGO_PKG_NAME"));
        let source = std::fs::read_to_string(&path).expect("readable");
        let unfinished: Vec<&str> = source
            .lines()
            .filter(|line| !line.trim_start().starts_with('#') && line.contains("(s)"))
            .collect();
        assert!(unfinished.is_empty(), "{language}: {unfinished:#?}");
    }
}

/// The plural is chosen by the number: English puts only 1 in `one`, French puts 0 there too.
/// This file's own process, so switching the language here changes nothing elsewhere.
#[test]
fn a_count_takes_the_form_its_language_gives_its_number() {
    use std::collections::HashMap;

    use heimdall_core::settings::Language;

    let say = |count: usize| {
        heimdall_ui::i18n::LOADER.get_args("ui-selection-count", HashMap::from([("count", count)]))
    };
    heimdall_ui::i18n::apply(Some(Language::English));
    assert_eq!(say(1), "1 item selected");
    assert_eq!(say(0), "0 items selected");
    assert_eq!(say(2), "2 items selected");
    heimdall_ui::i18n::apply(Some(Language::French));
    assert_eq!(say(0), "0 élément sélectionné");
    assert_eq!(say(1), "1 élément sélectionné");
    assert_eq!(say(2), "2 éléments sélectionnés");
    heimdall_ui::i18n::apply(Some(Language::English));
}
