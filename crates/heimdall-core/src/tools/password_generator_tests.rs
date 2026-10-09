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

//! The Password Generator's tests, after the C# `PasswordGeneratorViewModelTests`.

use super::*;
use crate::tools::password_rules::{AMBIGUOUS_CHARS, LAYOUT_UNSAFE_CHARS};

fn generator() -> PasswordGenerator {
    PasswordGenerator::new(None, "en")
}

/// `settings` applied with the generation held off, then one generation.
fn with(generator: &mut PasswordGenerator, settings: Vec<Setting>) {
    generator.suspend_regeneration();
    for setting in settings {
        generator.set(setting);
    }
    generator.resume_regeneration();
}

fn bits(generator: &PasswordGenerator) -> f64 {
    generator.strength().map_or(0.0, |strength| strength.bits)
}

#[test]
fn the_tool_opens_on_a_random_password_of_24() {
    let generator = generator();
    assert_eq!(generator.password().chars().count(), 24);
    assert_eq!(generator.history().len(), 1);
    assert_eq!(generator.batch().len(), 1);
    assert!(!generator.show_batch());
    assert_eq!(
        generator.strength().map(|strength| strength.level),
        Some(StrengthLevel::Strong)
    );
}

#[test]
fn classes_unticked_are_never_drawn() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::IncludeLowercase(false),
            Setting::IncludeDigits(false),
            Setting::IncludeSymbols(false),
            Setting::Length(64),
        ],
    );
    assert!(generator.password().chars().all(|c| c.is_ascii_uppercase()));
    with(
        &mut generator,
        vec![
            Setting::IncludeUppercase(false),
            Setting::IncludeDigits(true),
        ],
    );
    assert!(generator.password().chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn the_safety_switches_filter_what_they_forbid() {
    // As the C# RandomMode_SafetyFlags_FilterForbiddenCharacters.
    let mut generator = generator();
    generator.set(Setting::Length(128));
    generator.set(Setting::ExcludeAmbiguous(true));
    assert!(
        !generator
            .password()
            .chars()
            .any(|c| AMBIGUOUS_CHARS.contains(c))
    );
    with(
        &mut generator,
        vec![
            Setting::IncludeUppercase(false),
            Setting::IncludeLowercase(false),
            Setting::IncludeDigits(false),
            Setting::CliSafe(true),
        ],
    );
    assert!(!generator.password().is_empty());
    assert!(
        !generator
            .password()
            .chars()
            .any(|c| SHELL_DANGEROUS_CHARS.contains(c))
    );
    with(
        &mut generator,
        vec![
            Setting::CliSafe(false),
            Setting::IncludeUppercase(true),
            Setting::IncludeLowercase(true),
            Setting::IncludeSymbols(false),
            Setting::LayoutSafe(true),
        ],
    );
    assert!(
        !generator
            .password()
            .chars()
            .any(|c| LAYOUT_UNSAFE_CHARS.contains(c))
    );
}

#[test]
fn no_class_ticked_makes_no_password() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::IncludeUppercase(false),
            Setting::IncludeLowercase(false),
            Setting::IncludeDigits(false),
            Setting::IncludeSymbols(false),
        ],
    );
    assert_eq!(generator.password(), "");
    assert!(generator.strength().is_none());
}

#[test]
fn every_class_ticked_appears_in_every_random_password() {
    // As the C# EverySelectedClass_AppearsInEveryRandomPassword.
    let mut generator = generator();
    for length in [4, 8, 12, 24] {
        generator.set(Setting::Length(length));
        for _ in 0..200 {
            generator.generate();
            let password = generator.password();
            assert!(password.chars().any(|c| c.is_ascii_uppercase()), "{length}");
            assert!(password.chars().any(|c| c.is_ascii_lowercase()), "{length}");
            assert!(password.chars().any(|c| c.is_ascii_digit()), "{length}");
            assert!(
                password.chars().any(|c| DEFAULT_SYMBOL_CHARS.contains(c)),
                "{length}"
            );
            assert!(
                generator
                    .issues()
                    .iter()
                    .all(|issue| *issue != Issue::NoDigit)
            );
        }
    }
}

#[test]
fn a_length_too_short_for_the_promise_says_so() {
    // As the C# ALengthTooShortForThePromise_SaysSoInsteadOfPretending.
    let mut generator = generator();
    generator.set(Setting::Length(3));
    assert!(generator.notices().contains(&Notice::ClassesNotPromised));
    let expected = 92_f64.log2() * 3.0;
    assert!((generator.last_entropy_bits() - expected).abs() < 1e-9);
}

#[test]
fn the_characters_drawn_are_spread_evenly_over_the_charset() {
    // Digits only, ten thousand passwords of 64: each digit within 3% of a tenth.
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::IncludeUppercase(false),
            Setting::IncludeLowercase(false),
            Setting::IncludeSymbols(false),
            Setting::Length(64),
            Setting::BatchCount(20),
        ],
    );
    let mut counts = [0_u32; 10];
    for _ in 0..500 {
        generator.generate();
        for password in generator.batch() {
            for digit in password.chars() {
                counts[usize::from(u8::try_from(digit).expect("ASCII") - b'0')] += 1;
            }
        }
    }
    let total: u32 = counts.iter().sum();
    assert_eq!(total, 500 * 20 * 64);
    for count in counts {
        let share = f64::from(count) / f64::from(total);
        assert!((share - 0.1).abs() < 0.003, "{counts:?}");
    }
}

#[test]
fn a_syllable_password_has_its_length_its_extras_and_its_structure() {
    // As the C# SyllableMode_CvcAndExtras_UpdateStructureAndPassword and
    // SyllableLength_IsTheLengthOfTheWholePassword.
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Syllable),
            Setting::SyllableLength(18),
            Setting::SyllableDigits(2),
            Setting::SyllableSpecials(2),
            Setting::SyllableCvc(true),
        ],
    );
    let mut saw_closed = false;
    for _ in 0..40 {
        generator.generate();
        let password = generator.password();
        assert_eq!(password.chars().count(), 18);
        assert!(password.chars().filter(char::is_ascii_digit).count() >= 2);
        assert!(password.chars().any(|c| DEFAULT_SYMBOL_CHARS.contains(c)));
        let structure = generator.syllable_structure();
        assert!(structure.ends_with("  + 2# 2!"), "{structure}");
        saw_closed |= structure
            .split("  + ")
            .next()
            .is_some_and(|groups| groups.split(" \u{b7} ").any(|group| group.len() == 3));
    }
    assert!(saw_closed);
    assert_eq!(generator.syllable_total_length(), 18);
    with(
        &mut generator,
        vec![
            Setting::SyllableLength(12),
            Setting::SyllableDigits(0),
            Setting::SyllableSpecials(0),
            Setting::SyllableSeparator("-".to_owned()),
            Setting::SyllableCvc(false),
        ],
    );
    assert_eq!(generator.password().chars().count(), 12);
}

#[test]
fn syllable_counts_give_way_to_the_length_and_say_so() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Syllable),
            Setting::SyllableLength(8),
            Setting::SyllableDigits(6),
            Setting::SyllableSpecials(6),
        ],
    );
    assert_eq!(generator.password().chars().count(), 8);
    assert!(generator.notices().contains(&Notice::CountsCut {
        digits: 3,
        specials: 3
    }));
}

#[test]
fn a_passphrase_takes_its_separator_its_case_and_its_language() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Passphrase),
            Setting::PassphraseWordCount(4),
            Setting::PassphraseSeparator("-".to_owned()),
            Setting::PassphraseDigits(0),
            Setting::PassphraseSpecials(0),
            Setting::PassphraseCase(CaseMode::WordCase),
            Setting::PassphraseLanguage(1),
        ],
    );
    let french = password_wordlists::word_list(1);
    let words: Vec<&str> = generator.password().split('-').collect();
    assert_eq!(words.len(), 4);
    for word in &words {
        assert!(
            word.chars().next().is_some_and(char::is_uppercase),
            "{word}"
        );
        assert!(french.contains(&word.to_lowercase()), "{word}");
    }
    let unique: std::collections::HashSet<&&str> = words.iter().collect();
    assert_eq!(unique.len(), 4, "each word once");
    let expected = as_f64(french.len()).log2() * 4.0;
    assert!((bits(&generator) - expected).abs() < 1e-9);
}

#[test]
fn a_passphrase_takes_as_many_digits_and_specials_as_asked() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Passphrase),
            Setting::PassphraseDigits(3),
            Setting::PassphraseSpecials(2),
            Setting::PassphraseSeparator(String::new()),
            Setting::PassphrasePlacement(Placement::End),
        ],
    );
    let tail: String = generator.password().chars().rev().take(5).collect();
    assert_eq!(tail.chars().filter(char::is_ascii_digit).count(), 3);
    assert_eq!(
        tail.chars()
            .filter(|c| DEFAULT_SYMBOL_CHARS.contains(*c))
            .count(),
        2
    );
}

#[test]
fn the_leet_table_rewrites_every_letter_it_covers() {
    // As the C# LeetMode_RewritesEveryLetterTheTableCovers and its CLI-safe sibling.
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Leet),
            Setting::LeetRandomWord(false),
            Setting::LeetBaseWord("abegilostqu".to_owned()),
            Setting::LeetFullSubstitution(true),
            Setting::LeetCase(CaseMode::Lower),
            Setting::LeetDigits(0),
            Setting::LeetSpecials(0),
        ],
    );
    assert_eq!(generator.password(), "@8391!057qu");
    with(
        &mut generator,
        vec![
            Setting::LeetBaseWord("ball".to_owned()),
            Setting::CliSafe(true),
        ],
    );
    assert_eq!(generator.password(), "8@ll");
}

#[test]
fn a_typed_leet_word_is_worth_nothing_and_a_partial_rewrite_a_bit_per_letter() {
    // As the C# LeetMode_CreditsNothingForAWordTheOperatorTyped and
    // LeetMode_PaysForSubstitutionsOnlyWhenTheyAreNotAllApplied.
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Leet),
            Setting::LeetRandomWord(false),
            Setting::LeetBaseWord("abc".to_owned()),
            Setting::LeetCase(CaseMode::Lower),
            Setting::LeetDigits(0),
            Setting::LeetSpecials(0),
        ],
    );
    assert!(bits(&generator).abs() < f64::EPSILON);
    assert!(generator.issues().contains(&Issue::ChosenWord));
    generator.set(Setting::LeetFullSubstitution(false));
    assert!((bits(&generator) - 2.0).abs() < f64::EPSILON);
    generator.set(Setting::LeetRandomWord(true));
    assert!(!generator.issues().contains(&Issue::ChosenWord));
    assert!(!generator.leet_word_source().is_empty());
}

#[test]
fn layout_safe_is_offered_where_the_generator_chooses_its_letters() {
    let mut generator = generator();
    for (mode, shown) in [
        (GeneratorMode::Random, true),
        (GeneratorMode::Syllable, true),
        (GeneratorMode::Passphrase, false),
        (GeneratorMode::Leet, false),
    ] {
        generator.set(Setting::Mode(mode));
        assert_eq!(generator.show_layout_safe(), shown);
    }
}

#[test]
fn the_minimum_generates_at_the_smallest_length_that_carries_it() {
    // As the C# EntropyFloor_GeneratesAtTheSmallestSizeThatCarriesTheFloor.
    for (index, floor) in [(1, 60.0), (2, 80.0), (3, 100.0), (4, 128.0)] {
        let mut generator = generator();
        generator.set(Setting::Length(4));
        generator.set(Setting::EntropyFloor(index));
        let length = generator.effective_length();
        assert!(generator.last_entropy_bits() >= floor);
        let per_character = generator.last_entropy_bits() / as_f64(length);
        assert!(per_character * as_f64(length - 1) < floor);
        assert_eq!(generator.password().chars().count(), length);
        assert_eq!(generator.settings().length, length, "the search wrote it");
        assert!(generator.floor_search_notice().is_some());
    }
}

#[test]
fn the_minimum_is_undone_when_it_is_lowered_or_cleared() {
    // As the C# EntropyFloor_LeavesTheOperatorsOwnSettingAloneAndIsUndoneByClearingIt.
    let mut generator = generator();
    generator.set(Setting::Length(6));
    generator.set(Setting::EntropyFloor(4));
    assert!(generator.settings().length > 6);
    generator.set(Setting::EntropyFloor(1));
    let at_sixty = generator.settings().length;
    assert!(at_sixty < 128);
    generator.set(Setting::EntropyFloor(4));
    assert!(generator.settings().length > at_sixty);
    generator.set(Setting::EntropyFloor(0));
    assert_eq!(generator.settings().length, 6);
    assert_eq!(generator.password().chars().count(), 6);
    assert!(generator.notices().is_empty());
    assert!(generator.floor_search_notice().is_none());
}

#[test]
fn a_minimum_out_of_reach_changes_nothing_and_says_how_far_it_gets() {
    // As the C# EntropyFloor_OutOfReach_ChangesNothingAndSaysSo.
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Length(10),
            Setting::IncludeUppercase(false),
            Setting::IncludeLowercase(false),
            Setting::IncludeDigits(false),
            Setting::CustomSpecials("!".to_owned()),
        ],
    );
    generator.set(Setting::EntropyFloor(1));
    assert_eq!(generator.settings().length, 10);
    assert_eq!(generator.password().chars().count(), 10);
    assert!(generator.issues().contains(&Issue::FloorUnreachable {
        floor: 60,
        ceiling: 0
    }));
}

#[test]
fn the_minimum_gives_a_passphrase_words_and_a_leet_password_digits_first() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Passphrase),
            Setting::PassphraseWordCount(2),
            Setting::PassphraseDigits(0),
            Setting::PassphraseSpecials(0),
            Setting::PassphraseCase(CaseMode::Lower),
        ],
    );
    generator.set(Setting::EntropyFloor(2));
    assert!(generator.last_entropy_bits() >= 80.0);
    assert!(generator.settings().passphrase_word_count > 2);
    assert_eq!(
        generator.settings().passphrase_digits,
        0,
        "words before extras"
    );
    assert_eq!(
        generator.password().split('-').count(),
        generator.settings().passphrase_word_count
    );

    let mut leet = PasswordGenerator::new(None, "en");
    with(
        &mut leet,
        vec![
            Setting::Mode(GeneratorMode::Leet),
            Setting::LeetDigits(0),
            Setting::LeetSpecials(0),
            Setting::LeetCase(CaseMode::Lower),
        ],
    );
    leet.set(Setting::EntropyFloor(1));
    assert_eq!(leet.settings().leet_digits, 6);
    assert!(leet.settings().leet_specials > 0);
    assert!(leet.last_entropy_bits() >= 60.0);
}

#[test]
fn the_minimum_gives_a_syllable_password_more_syllables() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Syllable),
            Setting::SyllableLength(8),
            Setting::SyllableDigits(0),
            Setting::SyllableSpecials(0),
            Setting::SyllableCase(CaseMode::Lower),
        ],
    );
    generator.set(Setting::EntropyFloor(1));
    let length = generator.settings().syllable_length;
    assert!(length > 8 && length.is_multiple_of(2));
    assert!(generator.last_entropy_bits() >= 60.0);
    assert_eq!(generator.password().chars().count(), length);
}

#[test]
fn case_blocks_case_one_syllable_each_and_are_worth_nothing() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Syllable),
            Setting::SyllableLength(12),
            Setting::SyllableDigits(0),
            Setting::SyllableSpecials(0),
            Setting::SyllableCase(CaseMode::Blocks),
            Setting::CaseBlocksAutoSync(false),
            Setting::CaseBlocks("Ul".to_owned()),
        ],
    );
    let structure = generator.syllable_structure().to_owned();
    for (index, group) in structure.split(" \u{b7} ").enumerate() {
        if index % 2 == 0 {
            assert_eq!(group, group.to_uppercase(), "{structure}");
        } else {
            assert_eq!(group, group.to_lowercase(), "{structure}");
        }
    }
    // Six syllables of 19 by 6, nothing for the case.
    let expected = 114_f64.log2() * 6.0;
    assert!((bits(&generator) - expected).abs() < 1e-9);
}

#[test]
fn the_block_editor_stays_within_its_bounds_and_follows_the_syllables() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Syllable),
            Setting::SyllableCase(CaseMode::Blocks),
        ],
    );
    // Synced: 16 characters, 2 digits and 1 special leave 13, six syllables.
    assert_eq!(generator.settings().case_blocks.len(), 6);
    generator.set(Setting::SyllableLength(20));
    assert_eq!(generator.settings().case_blocks.len(), 8);
    for _ in 0..20 {
        generator.add_case_block();
    }
    assert_eq!(generator.settings().case_blocks.len(), 10);
    assert!(!generator.settings().case_blocks_auto_sync);
    for _ in 0..20 {
        generator.remove_case_block();
    }
    assert_eq!(generator.settings().case_blocks, "T");
    generator.cycle_case_block(0);
    assert_eq!(generator.settings().case_blocks, "U");
    generator.cycle_case_block(0);
    assert_eq!(generator.settings().case_blocks, "l");
    generator.add_case_block();
    generator.set_all_case_blocks('T');
    assert_eq!(generator.settings().case_blocks, "TT");
    generator.set_all_case_blocks('x');
    assert_eq!(generator.settings().case_blocks, "TT");
    generator.randomize_case_blocks();
    assert!(
        generator
            .settings()
            .case_blocks
            .chars()
            .all(|c| CASE_BLOCK_TOKENS.contains(&c))
    );
}

#[test]
fn a_cursor_moves_the_character_already_there_without_drawing_another() {
    // As the C# MovingACursor_MovesTheCharacterThatIsThere_WithoutDrawingANewPassword.
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Syllable),
            Setting::SyllableLength(12),
            Setting::SyllableDigits(1),
            Setting::SyllableSpecials(0),
            Setting::SyllablePlacement(Placement::Positions),
            Setting::DigitPositions("0".to_owned()),
        ],
    );
    assert!(generator.show_placement_bar());
    let before = generator.password().to_owned();
    assert!(
        before.chars().next().is_some_and(|c| c.is_ascii_digit()),
        "{before}"
    );
    let changes = generator.settings_changes();
    assert!(generator.try_move_in_place(true, 0, 100.0, false));
    let moved = generator.password().to_owned();
    assert!(
        moved.chars().last().is_some_and(|c| c.is_ascii_digit()),
        "{moved}"
    );
    let letters = |text: &str| {
        text.chars()
            .filter(|c| !c.is_ascii_digit())
            .collect::<String>()
    };
    assert_eq!(letters(&before), letters(&moved), "the same letters");
    assert_eq!(
        generator.settings().digit_positions,
        "0",
        "a preview writes nothing"
    );
    assert!(generator.try_move_in_place(true, 0, 100.0, true));
    assert_eq!(generator.settings().digit_positions, "100");
    assert_eq!(generator.password(), moved);
    assert_eq!(generator.history()[0].as_str(), moved);
    assert!(generator.settings_changes() > changes);
    assert!(
        !generator.try_move_in_place(true, 5, 50.0, true),
        "no such cursor"
    );
}

#[test]
fn a_batch_has_its_count_shows_its_first_and_adds_only_it_to_the_history() {
    let mut generator = generator();
    generator.set(Setting::BatchCount(5));
    assert_eq!(generator.batch().len(), 5);
    assert!(generator.show_batch());
    assert_eq!(generator.batch()[0].as_str(), generator.password());
    let history = generator.history().len();
    generator.generate();
    assert_eq!(generator.history().len(), history + 1);
    generator.set(Setting::MaskBatch(true));
    let rows = generator.batch_rows();
    assert!(
        rows.iter()
            .all(|row| row.chars().all(|c| c == MASK_CHARACTER))
    );
    assert_eq!(rows[0].chars().count(), 24);
    assert_eq!(generator.batch_text("\n").lines().count(), 5);
    generator.set(Setting::BatchCount(1));
    assert!(!generator.show_batch());
}

#[test]
fn the_history_keeps_the_last_ten() {
    let mut generator = generator();
    for _ in 0..15 {
        generator.generate();
    }
    assert_eq!(generator.history().len(), HISTORY_MAX_SIZE);
    assert_eq!(generator.history()[0].as_str(), generator.password());
    generator.clear_history();
    assert!(generator.history().is_empty());
    generator.clear_output();
    assert!(generator.password().is_empty() && generator.batch().is_empty());
}

#[test]
fn a_preset_round_trips_every_setting() {
    let mut generator = generator();
    with(
        &mut generator,
        vec![
            Setting::Mode(GeneratorMode::Passphrase),
            Setting::Length(40),
            Setting::PassphraseWordCount(6),
            Setting::PassphraseSeparator("_".to_owned()),
            Setting::PassphraseDigits(3),
            Setting::LeetBaseWord("dragon".to_owned()),
            Setting::CaseBlocks("UlT".to_owned()),
            Setting::BatchCount(3),
            Setting::SyllableSeparator(".".to_owned()),
        ],
    );
    let preset = generator.snapshot("Mine");
    let mut other = PasswordGenerator::new(None, "en");
    other.apply_preset(&preset);
    assert_eq!(other.snapshot("Mine"), preset);
    assert_eq!(other.settings().mode, GeneratorMode::Passphrase);
    assert_eq!(other.batch().len(), 3);
}

#[test]
fn an_older_preset_keeps_what_it_meant() {
    // As the C# ASyllablePresetWrittenBeforeTheChange_HasItsCountsAddedBack and
    // PassphrasePreset_WrittenBeforeTheCounts_KeepsWhatItMeant.
    let preset: PasswordPreset = serde_json::from_str(
        r#"{"Mode":1,"SylLength":16,"SylDigits":2,"SylSpecials":1,"PpDigit":false,"PpSpecial":true,"PpCapitalize":false}"#,
    )
    .expect("read");
    let mut generator = generator();
    generator.apply_preset(&preset);
    assert_eq!(generator.settings().syllable_length, 19);
    assert_eq!(generator.settings().passphrase_digits, 0);
    assert_eq!(generator.settings().passphrase_specials, 1);
    assert_eq!(generator.settings().passphrase_case, CaseMode::Lower);
}

#[test]
fn presets_are_saved_by_mode_and_the_settings_remembered_only_when_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(password_presets::PRESETS_FILE_NAME);
    let mut generator = PasswordGenerator::new(Some(path.clone()), "en");
    generator.save_preset("Mine");
    generator.save_preset("mine");
    assert_eq!(generator.preset_count(), 1, "one name whatever its case");
    generator.set(Setting::Mode(GeneratorMode::Syllable));
    assert!(generator.presets_for_current_mode().is_empty());
    assert_eq!(generator.preset_count(), 1, "saved in another mode");
    generator.set(Setting::Mode(GeneratorMode::Random));
    assert_eq!(generator.presets_for_current_mode().len(), 1);
    generator.delete_preset("MINE");
    assert_eq!(generator.preset_count(), 0);

    generator.set(Setting::Length(33));
    let reopened = PasswordGenerator::new(Some(path.clone()), "en");
    assert_eq!(
        reopened.settings().length,
        24,
        "not remembered unless asked"
    );
    generator.set(Setting::RememberSettings(true));
    generator.set(Setting::Length(40));
    generator.persist_settings_if_remembering();
    let reopened = PasswordGenerator::new(Some(path.clone()), "en");
    assert!(reopened.settings().remember_settings);
    assert_eq!(reopened.settings().length, 40);
    assert_eq!(reopened.password().chars().count(), 40);
    let written = std::fs::read_to_string(&path).expect("read");
    assert!(
        !written.contains(reopened.password()),
        "no password on disk"
    );
}

#[test]
fn the_interface_language_picks_the_passphrase_list() {
    assert_eq!(
        PasswordGenerator::new(None, "fr")
            .settings()
            .passphrase_language,
        1
    );
    assert_eq!(
        PasswordGenerator::new(None, "de")
            .settings()
            .passphrase_language,
        0
    );
    let summary = PasswordGenerator::new(None, "es").word_list_summary();
    let (words, bits) = summary.expect("a list");
    assert!(words > 700 && (bits - as_f64(words).log2()).abs() < 1e-9);
}

#[test]
fn the_specials_box_says_what_it_will_use() {
    let mut generator = generator();
    assert_eq!(generator.specials_notice(), None);
    generator.set(Setting::CustomSpecials("!a@".to_owned()));
    assert_eq!(
        generator.specials_notice(),
        Some(SpecialsNotice::Usable("!@".to_owned()))
    );
    generator.set(Setting::CustomSpecials("abc".to_owned()));
    assert_eq!(
        generator.specials_notice(),
        Some(SpecialsNotice::NoneUsable)
    );
}

#[test]
fn the_clipboard_delay_is_the_one_the_box_names() {
    let mut generator = generator();
    for (index, seconds) in [(0, 30), (1, 10), (2, 60), (3, 120), (9, 120)] {
        generator.set(Setting::ClipboardClearIndex(index));
        assert_eq!(generator.clipboard_clear_seconds(), seconds);
    }
}

#[test]
fn passwords_are_never_written_out() {
    let mut generator = generator();
    generator.set(Setting::LeetBaseWord("secretword".to_owned()));
    let shown = format!("{generator:?}");
    assert!(!shown.contains(generator.password()), "{shown}");
    assert!(!shown.contains("secretword"));
    assert!(!format!("{:?}", Setting::LeetBaseWord("secretword".to_owned())).contains("secret"));
}
