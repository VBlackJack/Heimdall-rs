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

/// Every text is in English, French and Spanish, as the C# Heimdall ships it: a key missing
/// from a language would show English to someone who chose another. A key English does not
/// have is refused too: it is dead, or a typo that would never be shown. So is a translation
/// naming other variables than the English: it would show an error in place of a value, or
/// leave the value out.
#[test]
fn every_language_holds_every_key_with_the_english_variables() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let gaps = heimdall_i18n::find_gaps(crate_root, env!("CARGO_PKG_NAME"))
        .expect("every language file exists and parses");
    let missing: Vec<_> = gaps
        .iter()
        .filter(|gap| gap.kind == GapKind::Missing)
        .collect();
    assert!(missing.is_empty(), "translations missing: {missing:#?}");
    assert!(gaps.is_empty(), "refused: {gaps:#?}");
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

/// The language files merge by union (`.gitattributes`): two branches adding keys at the end
/// keep both, without a conflict. The same key added by both would then be there twice, the
/// second one hiding the first: refused here.
#[test]
fn no_language_file_holds_a_key_twice() {
    let languages = Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n");
    let mut read = 0;
    for language in std::fs::read_dir(&languages).expect("languages") {
        let file = language
            .expect("language")
            .path()
            .join(concat!(env!("CARGO_PKG_NAME"), ".ftl"));
        let text = std::fs::read_to_string(&file).expect("language file");
        let mut seen = std::collections::BTreeSet::new();
        for line in text.lines() {
            // A message starts a line with its identifier; attributes, variants and
            // continuations are indented, comments start with #.
            let Some((id, _)) = line.split_once('=') else {
                continue;
            };
            let id = id.trim_end();
            let is_id = id.starts_with(|c: char| c.is_ascii_alphabetic())
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if is_id {
                assert!(seen.insert(id.to_owned()), "{}: {id} twice", file.display());
            }
        }
        assert!(!seen.is_empty(), "{}: no key read", file.display());
        read += 1;
    }
    assert!(read >= 3, "English, French and Spanish read, got {read}");
}
