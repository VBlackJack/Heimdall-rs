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

//! The Password Generator, as the C# `PasswordGeneratorViewModel`
//! (`PasswordGeneratorViewModel.cs:38-3603`): four modes (random, syllable, passphrase,
//! leet), each setting regenerating as the C#'s `OnXChanged` does; the minimum strength that
//! sizes a generation and the search that moves the settings to it, undone when it is
//! lowered; the case blocks; the placement bar, whose cursors move the characters already
//! drawn; a batch; the history of the last ten; the strength, the crack time, the issues and
//! the phonetic reading; the presets and the settings remembered.
//!
//! Every character is drawn from the system's generator, unbiased ([`SecureRandom`]).
//! Passwords are secrets: held in memory wiped when dropped or replaced, never written out
//! by `Debug`, never kept on disk. Only the settings are, in the presets file, and only when
//! the user saves a preset or asks for the settings to be remembered.

use std::fmt;
use std::path::PathBuf;

use zeroize::Zeroizing;

use super::password_presets::{self, PasswordGeneratorStore, PasswordPreset};
use super::password_rules::{
    self as rules, CASE_BLOCK_TOKENS, CaseMode, CrackTime, DEFAULT_CASE_BLOCKS,
    DEFAULT_SYMBOL_CHARS, DIGIT_CHARS, LEET_SUBSTITUTIONS, MAXIMUM_CASE_BLOCKS,
    MINIMUM_CASE_BLOCKS, MIXED_CASE_BITS_PER_LETTER, Placement, SHELL_DANGEROUS_CHARS, as_f64,
    utf16_length,
};
use super::password_wordlists;
use super::secure_random::SecureRandom;

/// The slider of a random password's length, as the C# `Minimum="4" Maximum="128"`.
pub const MINIMUM_LENGTH: usize = 4;
pub const MAXIMUM_LENGTH: usize = 128;

/// The slider of a syllable password's length, as the C# `Minimum="8" Maximum="32"`.
pub const MINIMUM_SYLLABLE_LENGTH: usize = 8;
pub const MAXIMUM_SYLLABLE_LENGTH: usize = 32;

/// The step of a syllable password's slider, as the C# `SyllableLengthStep`.
pub const SYLLABLE_LENGTH_STEP: usize = 2;

/// The slider of a passphrase's words, as the C# `Minimum="2" Maximum="8"`.
pub const MINIMUM_PASSPHRASE_WORDS: usize = 2;
pub const MAXIMUM_PASSPHRASE_WORDS: usize = 8;

/// Most digits and specials of a passphrase or a leet password, as the C# `MaximumLeetExtras`.
pub const MAXIMUM_LEET_EXTRAS: usize = 6;

/// Most digits and specials of a syllable password, as the C# `MaximumSyllableExtras`.
pub const MAXIMUM_SYLLABLE_EXTRAS: usize = 6;

/// How many passwords one click makes, as the C# `MinimumBatchCount` and `MaximumBatchCount`.
pub const MINIMUM_BATCH_COUNT: usize = 1;
pub const MAXIMUM_BATCH_COUNT: usize = 20;

/// Passwords the history keeps, as the C# `HistoryMaxSize`.
pub const HISTORY_MAX_SIZE: usize = 10;

/// The minimum strengths offered, in bits, the first meaning none, as the C#
/// `EntropyFloorChoices`.
pub const ENTROPY_FLOOR_CHOICES: [u32; 5] = [0, 60, 80, 100, 128];

/// Seconds a copied password stays on the clipboard, as the C# `ClipboardClearChoices`.
pub const CLIPBOARD_CLEAR_CHOICES: [u32; 4] = [30, 10, 60, 120];

/// The quick lengths of the random mode, as the C# buttons 8 to 64.
pub const QUICK_LENGTHS: [usize; 6] = [8, 12, 16, 24, 32, 64];

/// Draws of a random password before the promise of every class is given up, as the C#
/// `MaximumGuaranteeDraws`.
const MAXIMUM_GUARANTEE_DRAWS: usize = 10_000;

/// What a masked row shows for each character, as the C# `MaskCharacter`.
pub const MASK_CHARACTER: char = '\u{2022}';

/// What separates the syllables of the structure line, as the C#'s middle dot.
const STRUCTURE_SEPARATOR: &str = " \u{b7} ";

/// The strength's bounds and the bar's fill, as the C# `UpdateStrengthIndicator`.
const STRENGTH_BOUNDS: [(f64, f32); 4] = [(20.0, 0.10), (40.0, 0.25), (60.0, 0.50), (80.0, 0.75)];
const STRONG_FILL: f32 = 1.0;

/// A password shorter than this is said to be too short, as the C#'s `< 8`.
const SHORT_PASSWORD: usize = 8;

/// One chance in four of upper-casing a letter under mixed case, as the C#'s `GetInt32(4)`.
const MIXED_CASE_ODDS: usize = 4;

/// The generator's four modes, as the C# `GeneratorMode`; its index is kept in presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum GeneratorMode {
    /// Characters drawn from the classes ticked.
    #[default]
    Random,
    /// Pronounceable syllables.
    Syllable,
    /// Words of a list.
    Passphrase,
    /// One word rewritten in leet.
    Leet,
}

impl GeneratorMode {
    /// Every mode, in the box's order.
    pub const ALL: [Self; 4] = [Self::Random, Self::Syllable, Self::Passphrase, Self::Leet];

    /// The mode at `index`, the random one for an index no mode answers to, as the C#
    /// `CurrentMode`.
    #[must_use]
    pub fn at(index: i32) -> Self {
        match index {
            1 => Self::Syllable,
            2 => Self::Passphrase,
            3 => Self::Leet,
            _ => Self::Random,
        }
    }

    /// Its index, as presets keep it.
    #[must_use]
    pub const fn index(self) -> i32 {
        match self {
            Self::Random => 0,
            Self::Syllable => 1,
            Self::Passphrase => 2,
            Self::Leet => 3,
        }
    }
}

/// The tool's settings, as the C# view model's observable properties.
#[derive(Clone, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the C# tool's boxes, one switch each"
)]
pub struct Settings {
    pub mode: GeneratorMode,
    pub length: usize,
    pub include_uppercase: bool,
    pub include_lowercase: bool,
    pub include_digits: bool,
    pub include_symbols: bool,
    pub exclude_ambiguous: bool,
    pub cli_safe: bool,
    pub layout_safe: bool,
    pub custom_specials: String,
    pub syllable_length: usize,
    pub syllable_case: CaseMode,
    pub syllable_digits: usize,
    pub syllable_specials: usize,
    pub syllable_placement: Placement,
    pub syllable_separator: String,
    pub syllable_cvc: bool,
    pub passphrase_word_count: usize,
    pub passphrase_separator: String,
    pub passphrase_language: usize,
    pub passphrase_case: CaseMode,
    pub passphrase_digits: usize,
    pub passphrase_specials: usize,
    pub passphrase_placement: Placement,
    pub leet_base_word: String,
    pub leet_random_word: bool,
    pub leet_full_substitution: bool,
    pub leet_digits: usize,
    pub leet_specials: usize,
    pub leet_placement: Placement,
    pub leet_case: CaseMode,
    pub entropy_floor: usize,
    pub case_blocks: String,
    pub case_blocks_auto_sync: bool,
    pub digit_positions: String,
    pub special_positions: String,
    pub clipboard_auto_clear: bool,
    pub clipboard_clear_index: usize,
    pub batch_count: usize,
    pub mask_batch: bool,
    pub remember_settings: bool,
}

impl fmt::Debug for Settings {
    /// The leet base word, typed by the user, is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("mode", &self.mode)
            .field("length", &self.length)
            .field("syllable_length", &self.syllable_length)
            .field("passphrase_word_count", &self.passphrase_word_count)
            .field("entropy_floor", &self.entropy_floor)
            .field("batch_count", &self.batch_count)
            .finish_non_exhaustive()
    }
}

impl Default for Settings {
    /// The C# view model's initial values.
    fn default() -> Self {
        Self {
            mode: GeneratorMode::Random,
            length: 24,
            include_uppercase: true,
            include_lowercase: true,
            include_digits: true,
            include_symbols: true,
            exclude_ambiguous: false,
            cli_safe: false,
            layout_safe: false,
            custom_specials: DEFAULT_SYMBOL_CHARS.to_owned(),
            syllable_length: 16,
            syllable_case: CaseMode::Mixed,
            syllable_digits: 2,
            syllable_specials: 1,
            syllable_placement: Placement::Random,
            syllable_separator: String::new(),
            syllable_cvc: false,
            passphrase_word_count: 4,
            passphrase_separator: password_presets::DEFAULT_PASSPHRASE_SEPARATOR.to_owned(),
            passphrase_language: 0,
            passphrase_case: CaseMode::WordCase,
            passphrase_digits: 1,
            passphrase_specials: 1,
            passphrase_placement: Placement::Random,
            leet_base_word: String::new(),
            leet_random_word: true,
            leet_full_substitution: true,
            leet_digits: 2,
            leet_specials: 1,
            leet_placement: Placement::Random,
            leet_case: CaseMode::Mixed,
            entropy_floor: 0,
            case_blocks: DEFAULT_CASE_BLOCKS.to_owned(),
            case_blocks_auto_sync: true,
            digit_positions: String::new(),
            special_positions: String::new(),
            clipboard_auto_clear: false,
            clipboard_clear_index: 0,
            batch_count: MINIMUM_BATCH_COUNT,
            mask_batch: false,
            remember_settings: false,
        }
    }
}

/// A setting changed by the user, each as one C# property.
#[derive(Clone, PartialEq)]
pub enum Setting {
    Mode(GeneratorMode),
    Length(usize),
    IncludeUppercase(bool),
    IncludeLowercase(bool),
    IncludeDigits(bool),
    IncludeSymbols(bool),
    ExcludeAmbiguous(bool),
    CliSafe(bool),
    LayoutSafe(bool),
    CustomSpecials(String),
    SyllableLength(usize),
    SyllableCase(CaseMode),
    SyllableDigits(usize),
    SyllableSpecials(usize),
    SyllablePlacement(Placement),
    SyllableSeparator(String),
    SyllableCvc(bool),
    PassphraseWordCount(usize),
    PassphraseSeparator(String),
    PassphraseLanguage(usize),
    PassphraseCase(CaseMode),
    PassphraseDigits(usize),
    PassphraseSpecials(usize),
    PassphrasePlacement(Placement),
    LeetBaseWord(String),
    LeetRandomWord(bool),
    LeetFullSubstitution(bool),
    LeetDigits(usize),
    LeetSpecials(usize),
    LeetPlacement(Placement),
    LeetCase(CaseMode),
    EntropyFloor(usize),
    CaseBlocks(String),
    CaseBlocksAutoSync(bool),
    DigitPositions(String),
    SpecialPositions(String),
    ClipboardAutoClear(bool),
    ClipboardClearIndex(usize),
    BatchCount(usize),
    MaskBatch(bool),
    RememberSettings(bool),
}

impl fmt::Debug for Setting {
    /// The leet base word, typed by the user, is never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LeetBaseWord(_) => f.write_str("LeetBaseWord(..)"),
            Self::Mode(mode) => write!(f, "Mode({mode:?})"),
            other => {
                let name = format!("{:?}", std::mem::discriminant(other));
                f.write_str(&name)
            }
        }
    }
}

/// A sentence the notice line says of the generation, as the C# `FloorNoticeText`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The minimum made this password larger than the setting, as `ToolPwdGenFloorRaised`.
    FloorRaised {
        /// The setting.
        chosen: String,
        /// What was used.
        used: String,
    },
    /// The length had room for fewer digits and specials, as `ToolPwdGenCountsCut`.
    CountsCut {
        /// Digits it had room for.
        digits: usize,
        /// Specials it had room for.
        specials: usize,
    },
    /// Not every class could be promised, as `ToolPwdGenClassesNotPromised`.
    ClassesNotPromised,
}

/// What asking for a minimum did to the settings, as the C# `FloorSearchNoticeText`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorSearchNotice {
    /// The length set, as `ToolPwdGenFloorSetLength`.
    Length {
        /// The length.
        length: usize,
        /// The minimum, in bits.
        floor: u32,
    },
    /// A syllable password's shape set, as `ToolPwdGenFloorSetSyllable`.
    Syllable {
        /// Its length.
        length: usize,
        /// Its digits.
        digits: usize,
        /// Its specials.
        specials: usize,
        /// The minimum, in bits.
        floor: u32,
    },
    /// A passphrase's shape set, as `ToolPwdGenFloorSetPassphrase`.
    Passphrase {
        /// Its words.
        words: usize,
        /// Its digits.
        digits: usize,
        /// Its specials.
        specials: usize,
        /// The minimum, in bits.
        floor: u32,
    },
    /// A leet password's extras set, as `ToolPwdGenFloorSetLeet`.
    Leet {
        /// Its digits.
        digits: usize,
        /// Its specials.
        specials: usize,
        /// The minimum, in bits.
        floor: u32,
    },
}

/// An issue of the password shown, as the C# `UpdateIssuesList`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// Under eight characters.
    TooShort,
    /// The minimum cannot be guaranteed: it, and the most these settings reach.
    FloorUnreachable {
        /// The minimum, in bits.
        floor: u32,
        /// The most reached, in whole bits.
        ceiling: u32,
    },
    /// A word typed by the user.
    ChosenWord,
    /// No upper-case letter.
    NoUpper,
    /// No lower-case letter.
    NoLower,
    /// No digit.
    NoDigit,
    /// No special character.
    NoSpecial,
}

/// The strength's word, as the C# `ToolPwdGenStrength*` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrengthLevel {
    /// Under 20 bits.
    Critical,
    /// Under 40.
    Weak,
    /// Under 60.
    Fair,
    /// Under 80.
    Good,
    /// 80 and over.
    Strong,
}

/// The strength of the password shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strength {
    /// Its word.
    pub level: StrengthLevel,
    /// The bar's fill, from a tenth to one.
    pub fill: f32,
    /// Its bits.
    pub bits: f64,
}

/// What the custom specials box says under it, as the C# `CustomSpecialsNotice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecialsNotice {
    /// None of what was typed can be used.
    NoneUsable,
    /// Only these can.
    Usable(String),
}

/// What one password was made of, so its extras can be put elsewhere, as the C#
/// `PlacementMaterial`.
#[derive(Clone)]
struct PlacementMaterial {
    drawn: Zeroizing<Vec<char>>,
    digits: Zeroizing<Vec<char>>,
    specials: Zeroizing<Vec<char>>,
}

/// The settings a minimum may move, as they stood before it moved them, as the C#
/// `FloorUndo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FloorUndo {
    length: usize,
    syllable_length: usize,
    syllable_digits: usize,
    syllable_specials: usize,
    passphrase_word_count: usize,
    passphrase_digits: usize,
    passphrase_specials: usize,
    leet_digits: usize,
    leet_specials: usize,
}

/// What a search for the minimum would write, as the C# `FloorFound.Apply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FloorApply {
    Length(usize),
    Syllable(usize, usize, usize),
    Passphrase(usize, usize, usize),
    Leet(usize, usize),
}

/// What a search for the minimum found, as the C# `FloorFound`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FloorFound {
    reached: bool,
    ceiling: f64,
    apply: Option<FloorApply>,
}

impl FloorFound {
    const NOT_FOUND: Self = Self {
        reached: false,
        ceiling: 0.0,
        apply: None,
    };
}

/// The Password Generator's state, as the C# view model's.
pub struct PasswordGenerator {
    settings: Settings,
    initialized: bool,
    suspended: bool,
    store_path: Option<PathBuf>,
    store: Option<PasswordGeneratorStore>,
    random: SecureRandom,
    password: Zeroizing<String>,
    phonetic: Zeroizing<String>,
    strength: Option<Strength>,
    crack_time: Option<CrackTime>,
    issues: Vec<Issue>,
    syllable_structure: Zeroizing<String>,
    syllable_total_length: usize,
    leet_word_source: Zeroizing<String>,
    notices: Vec<Notice>,
    floor_search_notice: Option<FloorSearchNotice>,
    effective_length: usize,
    effective_syllable_length: usize,
    effective_syllable_digits: usize,
    effective_syllable_specials: usize,
    effective_passphrase_word_count: usize,
    effective_leet_digits: usize,
    effective_leet_specials: usize,
    floor_out_of_reach: bool,
    last_entropy_bits: f64,
    history: Vec<Zeroizing<String>>,
    batch: Vec<Zeroizing<String>>,
    batch_material: Vec<PlacementMaterial>,
    current_material: Option<PlacementMaterial>,
    material_password: Option<Zeroizing<String>>,
    before_floor: Option<FloorUndo>,
    settings_changes: u64,
    preset_changes: u64,
}

impl fmt::Debug for PasswordGenerator {
    /// The passwords are never written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasswordGenerator")
            .field("settings", &self.settings)
            .field("strength", &self.strength)
            .field("batch", &self.batch.len())
            .field("history", &self.history.len())
            .finish_non_exhaustive()
    }
}

/// Writes `$value` in `$field` and says whether it changed, as an observable property's
/// setter only calls its handler on a change.
macro_rules! changed {
    ($field:expr, $value:expr) => {{
        let value = $value;
        if $field == value {
            false
        } else {
            $field = value;
            true
        }
    }};
}

impl PasswordGenerator {
    /// The tool opened, as the C# `Initialize`: the passphrase language of the interface
    /// language `locale`, the settings remembered in the presets file at `store_path` when
    /// they are, then the first password. Without a path nothing is read nor written.
    #[must_use]
    pub fn new(store_path: Option<PathBuf>, locale: &str) -> Self {
        let mut generator = Self {
            settings: Settings::default(),
            initialized: false,
            suspended: false,
            store_path,
            store: None,
            random: SecureRandom::new(),
            password: Zeroizing::default(),
            phonetic: Zeroizing::default(),
            strength: None,
            crack_time: None,
            issues: Vec::new(),
            syllable_structure: Zeroizing::default(),
            syllable_total_length: 0,
            leet_word_source: Zeroizing::default(),
            notices: Vec::new(),
            floor_search_notice: None,
            effective_length: 0,
            effective_syllable_length: 0,
            effective_syllable_digits: 0,
            effective_syllable_specials: 0,
            effective_passphrase_word_count: 0,
            effective_leet_digits: 0,
            effective_leet_specials: 0,
            floor_out_of_reach: false,
            last_entropy_bits: 0.0,
            history: Vec::new(),
            batch: Vec::new(),
            batch_material: Vec::new(),
            current_material: None,
            material_password: None,
            before_floor: None,
            settings_changes: 0,
            preset_changes: 0,
        };
        generator.set(Setting::PassphraseLanguage(
            password_wordlists::language_index_for(locale),
        ));
        let store = generator.load_store().clone();
        if store.remember_settings {
            generator.settings.remember_settings = true;
            if let Some(remembered) = &store.settings {
                generator.apply_preset(remembered);
            }
        }
        generator.initialized = true;
        generator.generate();
        generator
    }

    // -- What the view reads --------------------------------------------------------------

    /// The settings.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The password shown.
    #[must_use]
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Its phonetic reading; empty past 32 characters.
    #[must_use]
    pub fn phonetic(&self) -> &str {
        &self.phonetic
    }

    /// Its strength; `None` without a password.
    #[must_use]
    pub fn strength(&self) -> Option<Strength> {
        self.strength
    }

    /// The time to crack it; `None` without a password.
    #[must_use]
    pub fn crack_time(&self) -> Option<CrackTime> {
        self.crack_time
    }

    /// Its issues.
    #[must_use]
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }

    /// What the notice line says of the generation.
    #[must_use]
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }

    /// What asking for a minimum did.
    #[must_use]
    pub fn floor_search_notice(&self) -> Option<FloorSearchNotice> {
        self.floor_search_notice
    }

    /// The syllables of a syllable password, as the structure line shows them.
    #[must_use]
    pub fn syllable_structure(&self) -> &str {
        &self.syllable_structure
    }

    /// The length of a syllable password.
    #[must_use]
    pub fn syllable_total_length(&self) -> usize {
        self.syllable_total_length
    }

    /// The word a leet password was drawn from; empty when it was typed.
    #[must_use]
    pub fn leet_word_source(&self) -> &str {
        &self.leet_word_source
    }

    /// The bits the last generation advertised.
    #[must_use]
    pub fn last_entropy_bits(&self) -> f64 {
        self.last_entropy_bits
    }

    /// The length the last random password was drawn at.
    #[must_use]
    pub fn effective_length(&self) -> usize {
        self.effective_length
    }

    /// The words the last passphrase was drawn with.
    #[must_use]
    pub fn effective_passphrase_word_count(&self) -> usize {
        self.effective_passphrase_word_count
    }

    /// The batch, the password shown first.
    #[must_use]
    pub fn batch(&self) -> &[Zeroizing<String>] {
        &self.batch
    }

    /// What the batch's list shows: the passwords, or a dot per character when masked.
    #[must_use]
    pub fn batch_rows(&self) -> Vec<Zeroizing<String>> {
        self.batch
            .iter()
            .map(|password| {
                if self.settings.mask_batch {
                    Zeroizing::new(
                        std::iter::repeat_n(MASK_CHARACTER, utf16_length(password)).collect(),
                    )
                } else {
                    password.clone()
                }
            })
            .collect()
    }

    /// The batch as it is copied and exported, one password per line, `new_line` between
    /// them, as the C# `BatchAsText`.
    #[must_use]
    pub fn batch_text(&self, new_line: &str) -> Zeroizing<String> {
        Zeroizing::new(
            self.batch
                .iter()
                .map(|password| password.as_str())
                .collect::<Vec<_>>()
                .join(new_line),
        )
    }

    /// Whether the batch's list is shown, as the C# `ShowBatch`.
    #[must_use]
    pub fn show_batch(&self) -> bool {
        self.batch.len() > 1
    }

    /// The last passwords shown, the newest first.
    #[must_use]
    pub fn history(&self) -> &[Zeroizing<String>] {
        &self.history
    }

    /// How many times the user changed a setting, as the C# `SettingsChanged` event: the
    /// preset list goes back to Custom when it moves.
    #[must_use]
    pub fn settings_changes(&self) -> u64 {
        self.settings_changes
    }

    /// How many times the presets saved changed, as the C# `CustomPresetsChanged`.
    #[must_use]
    pub fn preset_changes(&self) -> u64 {
        self.preset_changes
    }

    /// The size of the chosen language's list and the bits a word of it carries, as the C#
    /// `UpdateWordListSummary`; `None` under two words.
    #[must_use]
    pub fn word_list_summary(&self) -> Option<(usize, f64)> {
        let words = self.word_list();
        (words.len() >= 2).then(|| (words.len(), as_f64(words.len()).log2()))
    }

    /// What the specials box says under it, as the C# `CustomSpecialsNotice`.
    #[must_use]
    pub fn specials_notice(&self) -> Option<SpecialsNotice> {
        let typed = &self.settings.custom_specials;
        if typed.trim().is_empty() {
            return None;
        }
        let usable = rules::sanitize_custom_specials(typed);
        if usable == *typed {
            None
        } else if usable.is_empty() {
            Some(SpecialsNotice::NoneUsable)
        } else {
            Some(SpecialsNotice::Usable(usable))
        }
    }

    /// Seconds the clipboard keeps a copy, as the C# `ClipboardClearSeconds`.
    #[must_use]
    pub fn clipboard_clear_seconds(&self) -> u32 {
        CLIPBOARD_CLEAR_CHOICES[self
            .settings
            .clipboard_clear_index
            .min(CLIPBOARD_CLEAR_CHOICES.len() - 1)]
    }

    /// The minimum in bits, as the C# `EntropyFloorBits`.
    #[must_use]
    pub fn entropy_floor_bits(&self) -> u32 {
        ENTROPY_FLOOR_CHOICES[self
            .settings
            .entropy_floor
            .min(ENTROPY_FLOOR_CHOICES.len() - 1)]
    }

    /// The case mode of the mode shown, as the C# `CurrentCaseMode`.
    #[must_use]
    pub fn current_case_mode(&self) -> Option<CaseMode> {
        match self.settings.mode {
            GeneratorMode::Random => None,
            GeneratorMode::Syllable => Some(self.settings.syllable_case),
            GeneratorMode::Passphrase => Some(self.settings.passphrase_case),
            GeneratorMode::Leet => Some(self.settings.leet_case),
        }
    }

    /// The placement of the mode shown, as the C# `CurrentPlacement`.
    #[must_use]
    pub fn current_placement(&self) -> Option<Placement> {
        match self.settings.mode {
            GeneratorMode::Random => None,
            GeneratorMode::Syllable => Some(self.settings.syllable_placement),
            GeneratorMode::Passphrase => Some(self.settings.passphrase_placement),
            GeneratorMode::Leet => Some(self.settings.leet_placement),
        }
    }

    /// The digits the mode shown inserts, as the C# `CurrentDigitCount`.
    #[must_use]
    pub fn current_digit_count(&self) -> usize {
        match self.settings.mode {
            GeneratorMode::Random => 0,
            GeneratorMode::Syllable => self.settings.syllable_digits,
            GeneratorMode::Passphrase => self.settings.passphrase_digits,
            GeneratorMode::Leet => self.effective_leet_digits,
        }
    }

    /// The specials the mode shown inserts, as the C# `CurrentSpecialCount`.
    #[must_use]
    pub fn current_special_count(&self) -> usize {
        if self.effective_symbols().is_empty() {
            return 0;
        }
        match self.settings.mode {
            GeneratorMode::Random => 0,
            GeneratorMode::Syllable => self.settings.syllable_specials,
            GeneratorMode::Passphrase => self.settings.passphrase_specials,
            GeneratorMode::Leet => self.effective_leet_specials,
        }
    }

    /// Whether the placement bar is shown, as the C# `ShowPlacementBar`.
    #[must_use]
    pub fn show_placement_bar(&self) -> bool {
        self.current_placement() == Some(Placement::Positions)
            && self.current_digit_count() + self.current_special_count() > 0
    }

    /// Whether the case blocks are shown, as the C# `ShowCaseBlocks`.
    #[must_use]
    pub fn show_case_blocks(&self) -> bool {
        self.current_case_mode() == Some(CaseMode::Blocks)
    }

    /// Whether the blocks' switch to follow the units is shown, as the C#
    /// `ShowCaseBlocksAutoSync`.
    #[must_use]
    pub fn show_case_blocks_auto_sync(&self) -> bool {
        self.show_case_blocks()
            && matches!(
                self.settings.mode,
                GeneratorMode::Syllable | GeneratorMode::Passphrase
            )
    }

    /// Whether specials are drawn in the mode shown, as the C# `HasActiveSpecials`.
    #[must_use]
    pub fn has_active_specials(&self) -> bool {
        match self.settings.mode {
            GeneratorMode::Random => self.settings.include_symbols,
            GeneratorMode::Syllable => self.settings.syllable_specials > 0,
            GeneratorMode::Passphrase => self.settings.passphrase_specials > 0,
            GeneratorMode::Leet => self.effective_leet_specials > 0,
        }
    }

    /// Whether the layout-safe switch is shown, as the C# `ShowLayoutSafe`.
    #[must_use]
    pub fn show_layout_safe(&self) -> bool {
        matches!(
            self.settings.mode,
            GeneratorMode::Random | GeneratorMode::Syllable
        )
    }

    /// Whether the leet placement is shown, as the C# `ShowLeetPlacement`.
    #[must_use]
    pub fn show_leet_placement(&self) -> bool {
        self.settings.mode == GeneratorMode::Leet
            && (self.effective_leet_digits > 0 || self.effective_leet_specials > 0)
    }

    // -- What the user does -----------------------------------------------------------------

    /// Changes one setting, as the C# property's setter and its `OnXChanged`: nothing when
    /// the value is the one already set.
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per setting, as the C# properties"
    )]
    pub fn set(&mut self, setting: Setting) {
        let s = &mut self.settings;
        match setting {
            Setting::Mode(value) => {
                if changed!(s.mode, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::Length(value) => {
                if changed!(s.length, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::IncludeUppercase(value) => {
                if changed!(s.include_uppercase, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::IncludeLowercase(value) => {
                if changed!(s.include_lowercase, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::IncludeDigits(value) => {
                if changed!(s.include_digits, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::IncludeSymbols(value) => {
                if changed!(s.include_symbols, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::ExcludeAmbiguous(value) => {
                if changed!(s.exclude_ambiguous, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::CliSafe(value) => {
                if changed!(s.cli_safe, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::LayoutSafe(value) => {
                if changed!(s.layout_safe, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::CustomSpecials(value) => {
                if changed!(s.custom_specials, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::SyllableLength(value) => {
                if changed!(s.syllable_length, value) {
                    self.sync_case_blocks();
                    self.regenerate_if_ready();
                }
            }
            Setting::SyllableCase(value) => {
                if changed!(s.syllable_case, value) {
                    self.sync_case_blocks();
                    self.regenerate_if_ready();
                }
            }
            Setting::SyllableDigits(value) => {
                if changed!(s.syllable_digits, value) {
                    self.sync_case_blocks();
                    self.regenerate_if_ready();
                }
            }
            Setting::SyllableSpecials(value) => {
                if changed!(s.syllable_specials, value) {
                    self.sync_case_blocks();
                    self.regenerate_if_ready();
                }
            }
            Setting::SyllablePlacement(value) => {
                if changed!(s.syllable_placement, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::SyllableSeparator(value) => {
                if changed!(s.syllable_separator, value) {
                    self.sync_case_blocks();
                    self.regenerate_if_ready();
                }
            }
            Setting::SyllableCvc(value) => {
                if changed!(s.syllable_cvc, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::PassphraseWordCount(value) => {
                if changed!(s.passphrase_word_count, value) {
                    self.sync_case_blocks();
                    self.regenerate_if_ready();
                }
            }
            Setting::PassphraseSeparator(value) => {
                if changed!(s.passphrase_separator, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::PassphraseLanguage(value) => {
                if changed!(s.passphrase_language, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::PassphraseCase(value) => {
                if changed!(s.passphrase_case, value) {
                    self.sync_case_blocks();
                    self.regenerate_if_ready();
                }
            }
            Setting::PassphraseDigits(value) => {
                if changed!(s.passphrase_digits, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::PassphraseSpecials(value) => {
                if changed!(s.passphrase_specials, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::PassphrasePlacement(value) => {
                if changed!(s.passphrase_placement, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::LeetBaseWord(value) => {
                if changed!(s.leet_base_word, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::LeetRandomWord(value) => {
                if changed!(s.leet_random_word, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::LeetFullSubstitution(value) => {
                if changed!(s.leet_full_substitution, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::LeetDigits(value) => {
                if changed!(s.leet_digits, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::LeetSpecials(value) => {
                if changed!(s.leet_specials, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::LeetPlacement(value) => {
                if changed!(s.leet_placement, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::LeetCase(value) => {
                if changed!(s.leet_case, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::EntropyFloor(value) => {
                // Only when the user asks: a preset writes its own minimum with the
                // generation held off, and a search from there would undo the preset.
                if changed!(s.entropy_floor, value) && !self.suspended && self.initialized {
                    self.apply_floor_to_settings();
                }
            }
            Setting::CaseBlocks(value) => {
                if changed!(s.case_blocks, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::CaseBlocksAutoSync(value) => {
                if changed!(s.case_blocks_auto_sync, value) {
                    self.sync_case_blocks();
                    self.regenerate_if_ready();
                }
            }
            Setting::DigitPositions(value) => {
                if changed!(s.digit_positions, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::SpecialPositions(value) => {
                if changed!(s.special_positions, value) {
                    self.regenerate_if_ready();
                }
            }
            Setting::ClipboardAutoClear(value) => s.clipboard_auto_clear = value,
            Setting::ClipboardClearIndex(value) => s.clipboard_clear_index = value,
            Setting::BatchCount(value) => {
                if changed!(s.batch_count, value) {
                    self.regenerate_if_ready();
                }
            }
            // Masking hides what is shown; it asks for no other password.
            Setting::MaskBatch(value) => s.mask_batch = value,
            Setting::RememberSettings(value) => {
                if changed!(s.remember_settings, value) {
                    self.persist_settings_if_remembering();
                }
            }
        }
    }

    /// Another password, or batch, for the same settings, as the C# `GenerateCore`.
    pub fn generate(&mut self) {
        self.resolve_floor_sizes();
        self.resolve_position_counts();
        let wanted = self
            .settings
            .batch_count
            .clamp(MINIMUM_BATCH_COUNT, MAXIMUM_BATCH_COUNT);
        let mut extras = Vec::with_capacity(wanted - 1);
        let mut extra_material = Vec::with_capacity(wanted - 1);
        // The extras first and the password shown last, so its strength and its reading are
        // the ones on screen.
        for _ in 1..wanted {
            self.generate_for_current_mode();
            extras.push(self.password.clone());
            extra_material.push(self.current_material.take());
        }
        self.generate_for_current_mode();
        // Rearranged whole or not at all.
        self.batch_material.clear();
        self.material_password = None;
        if let Some(current) = self.current_material.take()
            && extra_material.iter().all(Option::is_some)
        {
            self.batch_material.push(current);
            self.batch_material
                .extend(extra_material.into_iter().flatten());
            self.material_password = Some(self.password.clone());
        }
        self.batch = std::iter::once(self.password.clone())
            .chain(extras)
            .collect();
        // Only the password shown enters the history.
        let shown = self.password.clone();
        self.add_to_history(&shown);
    }

    /// The output emptied, as the C# `ClearOutput` (Escape).
    pub fn clear_output(&mut self) {
        self.set_empty_output();
        self.batch.clear();
    }

    /// The history emptied, as the C# `ClearHistory`.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// One more block, up to ten, as the C# `AddCaseBlock`.
    pub fn add_case_block(&mut self) {
        if self.settings.case_blocks.chars().count() >= MAXIMUM_CASE_BLOCKS {
            return;
        }
        self.set(Setting::CaseBlocksAutoSync(false));
        let blocks = format!("{}{}", self.settings.case_blocks, CASE_BLOCK_TOKENS[1]);
        self.set(Setting::CaseBlocks(blocks));
    }

    /// One block fewer, down to one, as the C# `RemoveCaseBlock`.
    pub fn remove_case_block(&mut self) {
        if self.settings.case_blocks.chars().count() <= MINIMUM_CASE_BLOCKS {
            return;
        }
        self.set(Setting::CaseBlocksAutoSync(false));
        let mut blocks = self.settings.case_blocks.clone();
        blocks.pop();
        self.set(Setting::CaseBlocks(blocks));
    }

    /// A token drawn for every block, as the C# `RandomizeCaseBlocks`.
    pub fn randomize_case_blocks(&mut self) {
        let count = self.settings.case_blocks.chars().count();
        let mut drawn = String::with_capacity(count);
        for _ in 0..count {
            let Some(token) = self.random.pick(&CASE_BLOCK_TOKENS) else {
                return;
            };
            drawn.push(*token);
        }
        self.set(Setting::CaseBlocks(drawn));
    }

    /// Every block set to `token`, as the C# `SetAllCaseBlocks`.
    pub fn set_all_case_blocks(&mut self, token: char) {
        if !CASE_BLOCK_TOKENS.contains(&token) {
            return;
        }
        let count = self.settings.case_blocks.chars().count();
        self.set(Setting::CaseBlocks(
            std::iter::repeat_n(token, count).collect(),
        ));
    }

    /// Block `index` moved on to the next token, as the C# `CycleCaseBlock`.
    pub fn cycle_case_block(&mut self, index: usize) {
        let mut tokens: Vec<char> = self.settings.case_blocks.chars().collect();
        let Some(token) = tokens.get(index).copied() else {
            return;
        };
        let next = CASE_BLOCK_TOKENS
            .iter()
            .position(|known| *known == token)
            .map_or(0, |current| (current + 1) % CASE_BLOCK_TOKENS.len());
        tokens[index] = CASE_BLOCK_TOKENS[next];
        self.set(Setting::CaseBlocks(tokens.into_iter().collect()));
    }

    /// Every cursor spread out again, as the C# `DistributePositionsEvenly`.
    pub fn distribute_positions_evenly(&mut self) {
        let digits = rules::format_positions(&rules::distribute_evenly(self.current_digit_count()));
        let specials =
            rules::format_positions(&rules::distribute_evenly(self.current_special_count()));
        self.set(Setting::DigitPositions(digits));
        self.set(Setting::SpecialPositions(specials));
    }

    /// One cursor moved, written down, as the C# `MovePosition`.
    pub fn move_position(&mut self, digit: bool, index: usize, percent: f64) {
        let mut positions = rules::parse_positions(self.positions_text(digit));
        let Some(position) = positions.get_mut(index) else {
            return;
        };
        *position = rules::clamp_percent(percent);
        let text = rules::format_positions(&positions);
        self.set(if digit {
            Setting::DigitPositions(text)
        } else {
            Setting::SpecialPositions(text)
        });
    }

    /// One character moved in the passwords shown, those already drawn put back in a new
    /// order, as the C# `TryMoveInPlace`; written down when `commit`. `false` when there is
    /// nothing to move, or the material no longer made the password shown.
    pub fn try_move_in_place(
        &mut self,
        digit: bool,
        index: usize,
        percent: f64,
        commit: bool,
    ) -> bool {
        if self.batch_material.is_empty() || self.batch_material.len() != self.batch.len() {
            return false;
        }
        let mut positions = rules::parse_positions(self.positions_text(digit));
        if index >= positions.len() {
            return false;
        }
        if self.material_password.as_deref() != Some(&*self.password) {
            return false;
        }
        let current_digits = rules::parse_positions(&self.settings.digit_positions);
        let current_specials = rules::parse_positions(&self.settings.special_positions);
        positions[index] = rules::clamp_percent(percent);
        let text = rules::format_positions(&positions);
        let rebuilt: Vec<Zeroizing<String>> = self
            .batch_material
            .iter()
            .map(|material| {
                let (digits, specials) = if digit {
                    (&positions, &current_specials)
                } else {
                    (&current_digits, &positions)
                };
                rearrange(material, digits, specials)
            })
            .collect();
        if commit {
            // The setting written, the password kept as shown.
            self.suspended = true;
            self.set(if digit {
                Setting::DigitPositions(text)
            } else {
                Setting::SpecialPositions(text)
            });
            self.suspended = false;
        }
        self.batch.clone_from(&rebuilt);
        self.password.clone_from(&rebuilt[0]);
        self.material_password = Some(rebuilt[0].clone());
        self.phonetic = Zeroizing::new(rules::phonetic(&self.password));
        if commit {
            let shown = self.password.clone();
            self.add_to_history(&shown);
            self.raise_settings_changed();
        }
        true
    }

    /// The text of the digits' positions, or the specials'.
    fn positions_text(&self, digit: bool) -> &str {
        if digit {
            &self.settings.digit_positions
        } else {
            &self.settings.special_positions
        }
    }

    // -- Presets ----------------------------------------------------------------------------

    /// The settings as a preset named `name`, as the C# `SnapshotCurrentPreset`.
    #[must_use]
    pub fn snapshot(&self, name: &str) -> PasswordPreset {
        let s = &self.settings;
        let int = |value: usize| i32::try_from(value).unwrap_or(i32::MAX);
        PasswordPreset {
            name: name.to_owned(),
            mode: s.mode.index(),
            length: int(s.length),
            upper: s.include_uppercase,
            lower: s.include_lowercase,
            digits: s.include_digits,
            symbols: s.include_symbols,
            layout_safe: s.layout_safe,
            exclude_ambiguous: s.exclude_ambiguous,
            cli_safe: s.cli_safe,
            custom_specials: s.custom_specials.clone(),
            syl_length: int(s.syllable_length),
            syl_case: s.syllable_case.index(),
            syl_digits: int(s.syllable_digits),
            syl_specials: int(s.syllable_specials),
            syl_placement: s.syllable_placement.index(),
            syl_separator: s.syllable_separator.clone(),
            syl_cvc: s.syllable_cvc,
            syl_length_includes_extras: true,
            pp_word_count: int(s.passphrase_word_count),
            pp_separator: s.passphrase_separator.clone(),
            pp_language: int(s.passphrase_language),
            pp_capitalize: s.passphrase_case == CaseMode::WordCase,
            pp_digit: s.passphrase_digits > 0,
            pp_special: s.passphrase_specials > 0,
            pp_digits: int(s.passphrase_digits),
            pp_specials: int(s.passphrase_specials),
            pp_case: s.passphrase_case.index(),
            pp_placement: s.passphrase_placement.index(),
            leet_base_word: s.leet_base_word.clone(),
            leet_random_word: s.leet_random_word,
            leet_full_substitution: s.leet_full_substitution,
            leet_digits: int(s.leet_digits),
            leet_specials: int(s.leet_specials),
            leet_placement: s.leet_placement.index(),
            leet_case: s.leet_case.index(),
            entropy_floor: int(s.entropy_floor),
            case_blocks: s.case_blocks.clone(),
            case_blocks_auto_sync: s.case_blocks_auto_sync,
            digit_positions: s.digit_positions.clone(),
            special_positions: s.special_positions.clone(),
            batch_count: int(s.batch_count),
        }
    }

    /// Every setting of `preset` applied, then one generation, as the C# `ApplyPreset`.
    pub fn apply_preset(&mut self, preset: &PasswordPreset) {
        let count = |value: i32| usize::try_from(value).unwrap_or(0);
        self.suspended = true;
        self.set(Setting::Mode(GeneratorMode::at(preset.mode)));
        self.set(Setting::Length(count(preset.length)));
        self.set(Setting::IncludeUppercase(preset.upper));
        self.set(Setting::IncludeLowercase(preset.lower));
        self.set(Setting::IncludeDigits(preset.digits));
        self.set(Setting::IncludeSymbols(preset.symbols));
        self.set(Setting::LayoutSafe(preset.layout_safe));
        self.set(Setting::ExcludeAmbiguous(preset.exclude_ambiguous));
        self.set(Setting::CliSafe(preset.cli_safe));
        if !preset.custom_specials.is_empty() {
            self.set(Setting::CustomSpecials(preset.custom_specials.clone()));
        }
        // A preset written before the length covered the extras meant the letters alone.
        let syllable_length = if preset.syl_length_includes_extras {
            count(preset.syl_length)
        } else {
            (count(preset.syl_length) + count(preset.syl_digits) + count(preset.syl_specials))
                .min(MAXIMUM_SYLLABLE_LENGTH)
        };
        self.set(Setting::SyllableLength(syllable_length));
        self.set(Setting::SyllableCase(CaseMode::at(preset.syl_case)));
        self.set(Setting::SyllableDigits(count(preset.syl_digits)));
        self.set(Setting::SyllableSpecials(count(preset.syl_specials)));
        self.set(Setting::SyllablePlacement(Placement::at(
            preset.syl_placement,
        )));
        self.set(Setting::SyllableSeparator(preset.syl_separator.clone()));
        self.set(Setting::SyllableCvc(preset.syl_cvc));
        self.set(Setting::PassphraseWordCount(count(preset.pp_word_count)));
        self.set(Setting::PassphraseSeparator(preset.pp_separator.clone()));
        self.set(Setting::PassphraseLanguage(count(preset.pp_language)));
        // A preset written before the passphrase had counts carries the two flags only.
        let digits = if preset.pp_digits >= 0 {
            count(preset.pp_digits)
        } else {
            usize::from(preset.pp_digit)
        };
        let specials = if preset.pp_specials >= 0 {
            count(preset.pp_specials)
        } else {
            usize::from(preset.pp_special)
        };
        self.set(Setting::PassphraseDigits(digits));
        self.set(Setting::PassphraseSpecials(specials));
        let case = if preset.pp_case >= 0 {
            CaseMode::at(preset.pp_case)
        } else if preset.pp_capitalize {
            CaseMode::WordCase
        } else {
            CaseMode::Lower
        };
        self.set(Setting::PassphraseCase(case));
        self.set(Setting::PassphrasePlacement(Placement::at(
            preset.pp_placement,
        )));
        self.set(Setting::LeetBaseWord(preset.leet_base_word.clone()));
        self.set(Setting::LeetRandomWord(preset.leet_random_word));
        self.set(Setting::LeetFullSubstitution(preset.leet_full_substitution));
        self.set(Setting::LeetDigits(count(preset.leet_digits)));
        self.set(Setting::LeetSpecials(count(preset.leet_specials)));
        self.set(Setting::LeetPlacement(Placement::at(preset.leet_placement)));
        self.set(Setting::LeetCase(CaseMode::at(preset.leet_case)));
        self.set(Setting::EntropyFloor(count(preset.entropy_floor)));
        self.set(Setting::CaseBlocks(rules::sanitize_case_blocks(
            &preset.case_blocks,
        )));
        self.set(Setting::CaseBlocksAutoSync(preset.case_blocks_auto_sync));
        self.set(Setting::DigitPositions(rules::format_positions(
            &rules::parse_positions(&preset.digit_positions),
        )));
        self.set(Setting::SpecialPositions(rules::format_positions(
            &rules::parse_positions(&preset.special_positions),
        )));
        self.set(Setting::BatchCount(
            count(preset.batch_count).clamp(MINIMUM_BATCH_COUNT, MAXIMUM_BATCH_COUNT),
        ));
        self.resume();
    }

    /// The generation let go again, and run, as the C# `ResumeRegeneration`.
    fn resume(&mut self) {
        self.suspended = false;
        self.regenerate_if_ready();
    }

    /// The generation held off while several settings change, as the C#
    /// `SuspendRegeneration`.
    pub fn suspend_regeneration(&mut self) {
        self.suspended = true;
    }

    /// The generation let go again and run once, as the C# `ResumeRegeneration`.
    pub fn resume_regeneration(&mut self) {
        self.resume();
    }

    /// A built-in random preset, as the C# `ApplyRandomPreset`.
    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "the C# preset's four class boxes"
    )]
    pub fn apply_random_preset(
        &mut self,
        length: usize,
        upper: bool,
        lower: bool,
        digits: bool,
        symbols: bool,
    ) {
        self.suspended = true;
        self.set(Setting::Mode(GeneratorMode::Random));
        self.set(Setting::IncludeUppercase(upper));
        self.set(Setting::IncludeLowercase(lower));
        self.set(Setting::IncludeDigits(digits));
        self.set(Setting::IncludeSymbols(symbols));
        self.set(Setting::ExcludeAmbiguous(false));
        self.set(Setting::CliSafe(false));
        self.set(Setting::LayoutSafe(false));
        self.set(Setting::CustomSpecials(DEFAULT_SYMBOL_CHARS.to_owned()));
        self.set(Setting::Length(length));
        self.resume();
    }

    /// A built-in syllable preset, as the C# `ApplySyllablePreset`.
    pub fn apply_syllable_preset(
        &mut self,
        length: usize,
        case: CaseMode,
        digits: usize,
        specials: usize,
        separator: &str,
        cvc: bool,
    ) {
        self.suspended = true;
        self.set(Setting::Mode(GeneratorMode::Syllable));
        self.set(Setting::SyllableLength(length));
        self.set(Setting::SyllableCase(case));
        self.set(Setting::SyllableDigits(digits));
        self.set(Setting::SyllableSpecials(specials));
        self.set(Setting::SyllablePlacement(Placement::Random));
        self.set(Setting::SyllableSeparator(separator.to_owned()));
        self.set(Setting::SyllableCvc(cvc));
        self.set(Setting::LayoutSafe(false));
        self.resume();
    }

    /// A built-in passphrase preset, as the C# `ApplyPassphrasePreset`.
    pub fn apply_passphrase_preset(&mut self, word_count: usize, separator: &str) {
        self.suspended = true;
        self.set(Setting::Mode(GeneratorMode::Passphrase));
        self.set(Setting::PassphraseWordCount(word_count));
        self.set(Setting::PassphraseCase(CaseMode::WordCase));
        self.set(Setting::PassphraseDigits(1));
        self.set(Setting::PassphraseSpecials(1));
        self.set(Setting::PassphraseSeparator(separator.to_owned()));
        self.resume();
    }

    /// The store, read once.
    fn load_store(&mut self) -> &mut PasswordGeneratorStore {
        let path = self.store_path.clone();
        self.store.get_or_insert_with(|| {
            path.map_or_else(PasswordGeneratorStore::default, |path| {
                password_presets::load(&path)
            })
        })
    }

    /// The store written, when there is a file; a failure is let go, as the C# logs it.
    fn save_store(&mut self) {
        let Some(path) = self.store_path.clone() else {
            return;
        };
        let store = self.load_store().clone();
        let _ = password_presets::save(&path, &store);
    }

    /// The presets saved for the mode shown, as the C# `GetCustomPresetsForCurrentMode`.
    #[must_use]
    pub fn presets_for_current_mode(&self) -> Vec<PasswordPreset> {
        let mode = self.settings.mode.index();
        self.store
            .iter()
            .flat_map(|store| &store.presets)
            .filter(|preset| preset.mode == mode)
            .cloned()
            .collect()
    }

    /// How many presets are saved in all, as the C# `CustomPresetCount`.
    #[must_use]
    pub fn preset_count(&self) -> usize {
        self.store.as_ref().map_or(0, |store| store.presets.len())
    }

    /// The settings saved as preset `name`, replacing one of that name whatever its case, as
    /// the C# `SavePreset`.
    pub fn save_preset(&mut self, name: &str) {
        let preset = self.snapshot(name);
        let lowered = name.to_lowercase();
        let store = self.load_store();
        store
            .presets
            .retain(|known| known.name.to_lowercase() != lowered);
        store.presets.push(preset);
        self.save_store();
        self.preset_changes += 1;
    }

    /// Preset `name` deleted, whatever its case, as the C# `DeletePresetAsync` once it is
    /// confirmed.
    pub fn delete_preset(&mut self, name: &str) {
        let lowered = name.to_lowercase();
        self.load_store()
            .presets
            .retain(|known| known.name.to_lowercase() != lowered);
        self.save_store();
        self.preset_changes += 1;
    }

    /// Where the tool was left written down, or forgotten, as the C#
    /// `PersistSettingsIfRemembering`: run when the switch turns and when the tab closes.
    pub fn persist_settings_if_remembering(&mut self) {
        if !self.initialized {
            return;
        }
        let remember = self.settings.remember_settings;
        let snapshot = remember.then(|| self.snapshot(""));
        let store = self.load_store();
        store.remember_settings = remember;
        store.settings = snapshot;
        self.save_store();
    }

    // -- Generation -------------------------------------------------------------------------

    /// The settings moved by the user: the search's sentence and its way back let go, as the
    /// C# `RaiseSettingsChanged`.
    fn raise_settings_changed(&mut self) {
        self.floor_search_notice = None;
        self.before_floor = None;
        self.settings_changes += 1;
    }

    /// A generation, unless the tool is not ready or held off, as the C# `RegenerateIfReady`.
    fn regenerate_if_ready(&mut self) {
        if !self.initialized || self.suspended {
            return;
        }
        self.raise_settings_changed();
        self.generate();
    }

    /// The word list of the language chosen.
    fn word_list(&self) -> &'static [String] {
        password_wordlists::word_list(self.settings.passphrase_language)
    }

    /// The specials used, as the C# `GetEffectiveSymbols`.
    fn effective_symbols(&self) -> String {
        rules::effective_symbols(
            &self.settings.custom_specials,
            self.settings.mode == GeneratorMode::Random,
            self.settings.include_symbols,
            self.settings.cli_safe,
        )
    }

    /// The random mode's classes, as the C# `BuildCharsetClasses`.
    fn charset_classes(&self) -> Vec<Vec<char>> {
        let symbols = self.effective_symbols();
        let s = &self.settings;
        rules::charset_classes(
            s.include_uppercase,
            s.include_lowercase,
            s.include_digits,
            s.include_symbols.then_some(symbols.as_str()),
            s.exclude_ambiguous,
            s.layout_safe,
        )
    }

    fn generate_for_current_mode(&mut self) {
        self.current_material = None;
        let made = match self.settings.mode {
            GeneratorMode::Random => self.generate_random(),
            GeneratorMode::Syllable => self.generate_syllable(),
            GeneratorMode::Passphrase => self.generate_passphrase(),
            GeneratorMode::Leet => self.generate_leet(),
        };
        // The system's generator unreadable: no password rather than a weak one.
        if made.is_none() {
            self.current_material = None;
            self.set_empty_output();
        }
    }

    /// The sizes this generation runs at, as the C# `ResolveFloorSizes`.
    fn resolve_floor_sizes(&mut self) {
        self.effective_length = self.settings.length;
        self.effective_syllable_length = self.settings.syllable_length;
        self.effective_passphrase_word_count = self.settings.passphrase_word_count;
        self.effective_leet_digits = self.settings.leet_digits;
        self.effective_leet_specials = self.settings.leet_specials;
        self.floor_out_of_reach = false;
        self.notices.clear();
        let floor = f64::from(self.entropy_floor_bits());
        if floor > 0.0 {
            match self.settings.mode {
                GeneratorMode::Random => self.resolve_random_floor(floor),
                GeneratorMode::Syllable => self.resolve_syllable_floor(floor),
                GeneratorMode::Passphrase => self.resolve_passphrase_floor(floor),
                GeneratorMode::Leet => self.resolve_leet_floor(floor),
            }
        }
        // Last: a longer password may hold counts the shorter one did not.
        let (_, digits, specials) = rules::syllable_shape(
            self.effective_syllable_length,
            self.settings.syllable_digits,
            self.settings.syllable_specials,
        );
        self.effective_syllable_digits = digits;
        self.effective_syllable_specials = specials;
        if self.settings.mode == GeneratorMode::Syllable
            && (digits != self.settings.syllable_digits
                || specials != self.settings.syllable_specials)
        {
            self.append_notice(Notice::CountsCut { digits, specials });
        }
    }

    fn resolve_random_floor(&mut self, floor: f64) {
        let classes = self.charset_classes();
        if classes.is_empty() {
            self.floor_out_of_reach = true;
            return;
        }
        let length = self.settings.length;
        for candidate in length.max(1)..=MAXIMUM_LENGTH {
            if rules::guaranteed_random_bits(&classes, candidate) < floor {
                continue;
            }
            if candidate > length {
                self.effective_length = candidate;
                self.note_floor_raise(length.to_string(), candidate.to_string());
            }
            return;
        }
        self.floor_out_of_reach = true;
    }

    fn resolve_syllable_floor(&mut self, floor: f64) {
        let length = self.settings.syllable_length;
        let mut candidate = length;
        while candidate <= MAXIMUM_SYLLABLE_LENGTH {
            if self.guaranteed_syllable_bits(
                candidate,
                self.settings.syllable_digits,
                self.settings.syllable_specials,
            ) >= floor
            {
                if candidate > length {
                    self.effective_syllable_length = candidate;
                    self.note_floor_raise(length.to_string(), candidate.to_string());
                }
                return;
            }
            candidate += SYLLABLE_LENGTH_STEP;
        }
        self.floor_out_of_reach = true;
    }

    fn resolve_passphrase_floor(&mut self, floor: f64) {
        let words = self.settings.passphrase_word_count;
        for candidate in words..=MAXIMUM_PASSPHRASE_WORDS {
            if self.guaranteed_passphrase_bits(
                candidate,
                self.settings.passphrase_digits,
                self.settings.passphrase_specials,
            ) < floor
            {
                continue;
            }
            if candidate > words {
                self.effective_passphrase_word_count = candidate;
                self.note_floor_raise(words.to_string(), candidate.to_string());
            }
            return;
        }
        self.floor_out_of_reach = true;
    }

    /// Digits first, then specials, the fewest that carry the floor, as the C#
    /// `ResolveLeetFloor`.
    fn resolve_leet_floor(&mut self, floor: f64) {
        let (digits, specials) = (self.settings.leet_digits, self.settings.leet_specials);
        let digit_room = MAXIMUM_LEET_EXTRAS.saturating_sub(digits);
        let special_room = MAXIMUM_LEET_EXTRAS.saturating_sub(specials);
        for steps in 0..=digit_room + special_room {
            let more_digits = digits + steps.min(digit_room);
            let more_specials = specials + steps.saturating_sub(digit_room);
            if self.guaranteed_leet_bits(more_digits, more_specials) < floor {
                continue;
            }
            if steps > 0 {
                self.effective_leet_digits = more_digits;
                self.effective_leet_specials = more_specials;
                self.note_floor_raise(
                    format!("{digits}+{specials}"),
                    format!("{more_digits}+{more_specials}"),
                );
            }
            return;
        }
        self.floor_out_of_reach = true;
    }

    /// The bits a syllable password of this shape guarantees, as the C#
    /// `GuaranteedSyllableBits`.
    fn guaranteed_syllable_bits(&self, total: usize, digits: usize, specials: usize) -> f64 {
        let (consonants, vowels) = rules::syllable_letters(self.settings.layout_safe);
        let (_, digits_kept, specials_kept) = rules::syllable_shape(total, digits, specials);
        let count = rules::syllable_count(
            total,
            digits,
            specials,
            utf16_length(&self.settings.syllable_separator),
        );
        as_f64(consonants.len() * vowels.len()).log2() * as_f64(count)
            + rules::extras_bits(
                digits_kept,
                specials_kept,
                self.effective_symbols().chars().count(),
            )
    }

    /// The bits a passphrase of this shape guarantees, as the C# `GuaranteedPassphraseBits`.
    fn guaranteed_passphrase_bits(&self, words: usize, digits: usize, specials: usize) -> f64 {
        let list = self.word_list();
        if list.len() < 2 {
            return 0.0;
        }
        as_f64(list.len()).log2() * as_f64(words)
            + rules::extras_bits(digits, specials, self.effective_symbols().chars().count())
    }

    /// The bits a leet password guarantees whatever word is drawn, as the C#
    /// `GuaranteedLeetBits`.
    fn guaranteed_leet_bits(&self, digits: usize, specials: usize) -> f64 {
        let typed = sanitize_leet_base_word(&self.settings.leet_base_word);
        let drawn = self.settings.leet_random_word || typed.is_empty();
        let list = self.word_list();
        let word_bits = if drawn && list.len() > 1 {
            as_f64(list.len()).log2()
        } else {
            0.0
        };
        word_bits + rules::extras_bits(digits, specials, self.effective_symbols().chars().count())
    }

    fn note_floor_raise(&mut self, chosen: String, used: String) {
        self.notices.clear();
        self.notices.push(Notice::FloorRaised { chosen, used });
    }

    /// A sentence added to the notice line, once: a batch says it once, not once a password.
    fn append_notice(&mut self, notice: Notice) {
        if !self.notices.contains(&notice) {
            self.notices.push(notice);
        }
    }

    /// The position lists brought to the characters the mode inserts, as the C#
    /// `ResolvePositionCounts`: written with the generation held off.
    fn resolve_position_counts(&mut self) {
        let digits = rules::resize_positions(
            &rules::parse_positions(&self.settings.digit_positions),
            self.current_digit_count(),
        );
        let specials = rules::resize_positions(
            &rules::parse_positions(&self.settings.special_positions),
            self.current_special_count(),
        );
        let was = self.suspended;
        self.suspended = true;
        self.set(Setting::DigitPositions(rules::format_positions(&digits)));
        self.set(Setting::SpecialPositions(rules::format_positions(
            &specials,
        )));
        self.suspended = was;
    }

    fn generate_random(&mut self) -> Option<()> {
        self.syllable_structure = Zeroizing::default();
        self.syllable_total_length = 0;
        let classes = self.charset_classes();
        let charset: Vec<char> = classes.concat();
        if charset.is_empty() {
            self.set_empty_output();
            return Some(());
        }
        let length = self.effective_length;
        // Drawn again until every class ticked is there: a uniform draw over the passwords
        // that keep the promise.
        let promised = classes.len() <= length;
        let mut password = Zeroizing::new(Vec::with_capacity(length));
        let mut kept = false;
        for _ in 0..MAXIMUM_GUARANTEE_DRAWS {
            password.clear();
            for _ in 0..length {
                password.push(*self.random.pick(&charset)?);
            }
            if !promised {
                break;
            }
            if rules::carries_every_class(&password, &classes) {
                kept = true;
                break;
            }
        }
        self.password = Zeroizing::new(password.iter().collect());
        if !kept {
            self.append_notice(Notice::ClassesNotPromised);
        }
        let entropy = if kept {
            rules::guaranteed_random_bits(&classes, length)
        } else {
            as_f64(charset.len()).log2() * as_f64(length)
        };
        self.update_strength(entropy);
        self.update_phonetic();
        Some(())
    }

    fn generate_syllable(&mut self) -> Option<()> {
        let case = self.settings.syllable_case;
        let placement = self.settings.syllable_placement;
        let symbols: Vec<char> = self.effective_symbols().chars().collect();
        let separator = self.settings.syllable_separator.clone();
        let cvc = self.settings.syllable_cvc;
        let (consonants, vowels) = rules::syllable_letters(self.settings.layout_safe);
        let endings = rules::ending_consonants(self.settings.layout_safe);
        let digit_count = self.effective_syllable_digits;
        let special_count = self.effective_syllable_specials;
        let portion = self
            .effective_syllable_length
            .saturating_sub(digit_count + special_count);
        let separator_length = utf16_length(&separator);
        let mut groups: Vec<Zeroizing<String>> = Vec::new();
        let (mut written, mut cvc_count) = (0, 0);
        while written < portion {
            let cost = if groups.is_empty() {
                0
            } else {
                separator_length
            };
            let remaining = portion.saturating_sub(written + cost);
            if remaining < rules::CHARACTERS_PER_SYLLABLE_BLOCK {
                break;
            }
            let consonant = *self.random.pick(consonants)?;
            let vowel = *self.random.pick(vowels)?;
            if cvc && remaining >= 3 && self.random.below(2)? == 0 {
                let ending = *self.random.pick(&endings)?;
                groups.push(Zeroizing::new(format!("{consonant}{vowel}{ending}")));
                written += cost + 3;
                cvc_count += 1;
            } else {
                groups.push(Zeroizing::new(format!("{consonant}{vowel}")));
                written += cost + 2;
            }
        }
        // What is left, less than a syllable, closes the last one.
        while written < portion {
            let leftover = *self.random.pick(&endings)?;
            match groups.last_mut() {
                Some(last) => last.push_str(leftover),
                None => groups.push(Zeroizing::new(leftover.to_owned())),
            }
            written += 1;
        }
        self.case_syllables(&mut groups, case)?;
        let structure = groups
            .iter()
            .map(|group| group.as_str())
            .collect::<Vec<_>>()
            .join(STRUCTURE_SEPARATOR);
        let joined = Zeroizing::new(
            groups
                .iter()
                .map(|group| group.as_str())
                .collect::<Vec<_>>()
                .join(&separator),
        );
        let mut chars = Zeroizing::new(joined.chars().collect::<Vec<char>>());
        self.insert_extras(&mut chars, digit_count, special_count, placement, &symbols)?;
        self.password = Zeroizing::new(chars.iter().collect());
        self.syllable_total_length = utf16_length(&self.password);
        let structure = if digit_count > 0 || special_count > 0 {
            format!("{structure}  + {digit_count}# {special_count}!")
        } else {
            structure
        };
        self.syllable_structure = Zeroizing::new(structure);
        let open_pool = as_f64(consonants.len() * vowels.len());
        let closed_pool = as_f64(consonants.len() * vowels.len() * endings.len());
        let mut entropy = open_pool.log2() * as_f64(groups.len() - cvc_count)
            + closed_pool.log2() * as_f64(cvc_count);
        if cvc {
            entropy += as_f64(groups.len());
        }
        entropy += rules::extras_bits(digit_count, special_count, symbols.len());
        if case == CaseMode::Mixed {
            entropy += as_f64(groups.len());
        }
        self.update_strength(entropy);
        self.update_phonetic();
        Some(())
    }

    /// The syllables cased, as the C# `GenerateSyllablePassword`'s casing loop.
    fn case_syllables(&mut self, groups: &mut [Zeroizing<String>], case: CaseMode) -> Option<()> {
        let mut index = 0;
        for (group_index, group) in groups.iter_mut().enumerate() {
            if case == CaseMode::Blocks {
                index += group.chars().count();
                *group = Zeroizing::new(rules::apply_case_block(
                    group,
                    group_index,
                    &self.settings.case_blocks,
                ));
                continue;
            }
            let mut cased = Zeroizing::new(String::with_capacity(group.len()));
            for (position, character) in group.chars().enumerate() {
                let upper = match case {
                    CaseMode::Upper | CaseMode::Inverse => true,
                    CaseMode::Title => group_index == 0 && position == 0,
                    CaseMode::Mixed => self.random.below(MIXED_CASE_ODDS)? == 0,
                    CaseMode::Alternating => index % 2 != 0,
                    CaseMode::WordCase => position == 0,
                    CaseMode::Lower | CaseMode::Blocks => false,
                };
                if upper {
                    cased.extend(character.to_uppercase());
                } else {
                    cased.push(character);
                }
                index += 1;
            }
            *group = cased;
        }
        if case == CaseMode::Inverse
            && let Some(last) = groups.last_mut()
            && let Some(final_char) = last.pop()
        {
            last.extend(final_char.to_lowercase());
        }
        Some(())
    }

    /// The digits and specials drawn and put in place, as the C# `InsertExtras`.
    fn insert_extras(
        &mut self,
        chars: &mut Zeroizing<Vec<char>>,
        digit_count: usize,
        special_count: usize,
        placement: Placement,
        symbols: &[char],
    ) -> Option<()> {
        let digit_chars: Vec<char> = DIGIT_CHARS.chars().collect();
        let mut digits = Zeroizing::new(Vec::with_capacity(digit_count));
        for _ in 0..digit_count {
            digits.push(*self.random.pick(&digit_chars)?);
        }
        let mut specials = Zeroizing::new(Vec::new());
        if !symbols.is_empty() {
            for _ in 0..special_count {
                specials.push(*self.random.pick(symbols)?);
            }
        }
        if placement == Placement::Positions {
            // Kept so the cursors can move these characters without drawing others.
            self.current_material = Some(PlacementMaterial {
                drawn: chars.clone(),
                digits: digits.clone(),
                specials: specials.clone(),
            });
            rules::insert_at_positions(
                chars,
                &digits,
                &rules::parse_positions(&self.settings.digit_positions),
            );
            rules::insert_at_positions(
                chars,
                &specials,
                &rules::parse_positions(&self.settings.special_positions),
            );
            return Some(());
        }
        let extras: Vec<char> = digits.iter().chain(specials.iter()).copied().collect();
        match placement {
            Placement::Start => {
                chars.splice(0..0, extras);
            }
            Placement::End => chars.extend(extras),
            Placement::Middle => {
                let middle = chars.len() / 2;
                chars.splice(middle..middle, extras);
            }
            Placement::Random | Placement::Positions => {
                for extra in extras {
                    let at = self.random.below(chars.len() + 1)?;
                    chars.insert(at, extra);
                }
            }
        }
        Some(())
    }

    /// One word of a passphrase cased, as the C# `ApplyWordCase`.
    fn apply_word_case(&mut self, word: &str, word_index: usize) -> Option<String> {
        if word.is_empty() {
            return Some(String::new());
        }
        Some(match self.settings.passphrase_case {
            CaseMode::Blocks => {
                rules::apply_case_block(word, word_index, &self.settings.case_blocks)
            }
            CaseMode::Upper => word.to_uppercase(),
            CaseMode::Title | CaseMode::WordCase => rules::title_case(word),
            CaseMode::Alternating => {
                if word_index.is_multiple_of(2) {
                    word.to_lowercase()
                } else {
                    word.to_uppercase()
                }
            }
            CaseMode::Inverse => {
                if word_index + 1 == self.effective_passphrase_word_count {
                    let mut characters: Vec<char> = word.chars().collect();
                    let last = characters
                        .pop()
                        .map(|c| c.to_lowercase().collect::<String>());
                    characters.iter().collect::<String>().to_uppercase() + &last.unwrap_or_default()
                } else {
                    word.to_uppercase()
                }
            }
            CaseMode::Mixed => {
                let mut cased = String::with_capacity(word.len());
                for character in word.chars() {
                    if self.random.below(MIXED_CASE_ODDS)? == 0 {
                        cased.extend(character.to_uppercase());
                    } else {
                        cased.extend(character.to_lowercase());
                    }
                }
                cased
            }
            CaseMode::Lower => word.to_lowercase(),
        })
    }

    fn generate_passphrase(&mut self) -> Option<()> {
        self.syllable_structure = Zeroizing::default();
        self.syllable_total_length = 0;
        let list = self.word_list();
        if list.is_empty() {
            self.set_empty_output();
            return Some(());
        }
        let count = self.effective_passphrase_word_count;
        let mut used: Vec<usize> = Vec::with_capacity(count);
        let mut words: Vec<Zeroizing<String>> = Vec::with_capacity(count);
        for word_index in 0..count {
            let index = loop {
                let drawn = self.random.below(list.len())?;
                // Each word once, while the list has words not yet used.
                if used.len() >= list.len() || !used.contains(&drawn) {
                    break drawn;
                }
            };
            used.push(index);
            words.push(Zeroizing::new(
                self.apply_word_case(&list[index], word_index)?,
            ));
        }
        let joined = Zeroizing::new(
            words
                .iter()
                .map(|word| word.as_str())
                .collect::<Vec<_>>()
                .join(&self.settings.passphrase_separator),
        );
        let mut chars = Zeroizing::new(joined.chars().collect::<Vec<char>>());
        let symbols: Vec<char> = self.effective_symbols().chars().collect();
        let (digits, specials) = (
            self.settings.passphrase_digits,
            self.settings.passphrase_specials,
        );
        let placement = self.settings.passphrase_placement;
        self.insert_extras(&mut chars, digits, specials, placement, &symbols)?;
        self.password = Zeroizing::new(chars.iter().collect());
        let mut entropy = as_f64(list.len()).log2() * as_f64(count)
            + rules::extras_bits(digits, specials, symbols.len());
        if self.settings.passphrase_case == CaseMode::Mixed {
            let letters: usize = words
                .iter()
                .map(|word| word.chars().filter(|c| c.is_alphabetic()).count())
                .sum();
            entropy += MIXED_CASE_BITS_PER_LETTER * as_f64(letters);
        }
        self.update_strength(entropy);
        self.update_phonetic();
        Some(())
    }

    fn generate_leet(&mut self) -> Option<()> {
        self.syllable_structure = Zeroizing::default();
        self.syllable_total_length = 0;
        let typed = sanitize_leet_base_word(&self.settings.leet_base_word);
        let drawn = self.settings.leet_random_word || typed.is_empty();
        let (base, word_entropy) = if drawn {
            let list = self.word_list();
            if list.is_empty() {
                self.set_empty_output();
                return Some(());
            }
            let word = self.random.pick(list)?.clone();
            (Zeroizing::new(word), as_f64(list.len()).log2())
        } else {
            (typed, 0.0)
        };
        let symbols: Vec<char> = self.effective_symbols().chars().collect();
        let (substituted, substitutable) = self.apply_leet_substitutions(&base)?;
        let (cased, letters) = self.apply_leet_case(&substituted)?;
        let mut chars = Zeroizing::new(cased.chars().collect::<Vec<char>>());
        let (digits, specials) = (self.effective_leet_digits, self.effective_leet_specials);
        let placement = self.settings.leet_placement;
        self.insert_extras(&mut chars, digits, specials, placement, &symbols)?;
        self.password = Zeroizing::new(chars.iter().collect());
        self.leet_word_source = if drawn { base } else { Zeroizing::default() };
        let mut entropy = word_entropy;
        if !self.settings.leet_full_substitution {
            entropy += as_f64(substitutable);
        }
        if self.settings.leet_case == CaseMode::Mixed {
            entropy += MIXED_CASE_BITS_PER_LETTER * as_f64(letters);
        }
        entropy += rules::extras_bits(digits, specials, symbols.len());
        self.update_strength(entropy);
        self.update_phonetic();
        Some(())
    }

    /// The letters the table covers rewritten, all or one in two, and how many it could
    /// have rewritten, a shell's syntax skipped when CLI-safe, as the C#
    /// `ApplyLeetSubstitutions`.
    fn apply_leet_substitutions(&mut self, word: &str) -> Option<(Zeroizing<String>, usize)> {
        let mut substitutable = 0;
        let mut rewritten = Zeroizing::new(String::with_capacity(word.len()));
        for character in word.chars() {
            let lowered = character.to_lowercase().next().unwrap_or(character);
            let replacement = LEET_SUBSTITUTIONS
                .iter()
                .find(|(letter, _)| *letter == lowered)
                .map(|(_, replacement)| *replacement)
                .filter(|replacement| {
                    !(self.settings.cli_safe && SHELL_DANGEROUS_CHARS.contains(*replacement))
                });
            let Some(replacement) = replacement else {
                rewritten.push(character);
                continue;
            };
            substitutable += 1;
            let substitute = self.settings.leet_full_substitution || self.random.below(2)? == 0;
            rewritten.push(if substitute { replacement } else { character });
        }
        Some((rewritten, substitutable))
    }

    /// A leet word cased, and the letters it held, as the C# `ApplyLeetCase`.
    fn apply_leet_case(&mut self, word: &str) -> Option<(Zeroizing<String>, usize)> {
        let case = self.settings.leet_case;
        let letters = word.chars().filter(|c| c.is_alphabetic()).count();
        let mut cased = Zeroizing::new(String::with_capacity(word.len()));
        let mut block = 0;
        for (index, character) in word.chars().enumerate() {
            if case == CaseMode::Blocks {
                if character.is_alphabetic() {
                    cased.push_str(&rules::apply_case_block(
                        &character.to_string(),
                        block,
                        &self.settings.case_blocks,
                    ));
                    block += 1;
                } else {
                    cased.push(character);
                }
                continue;
            }
            let upper = match case {
                CaseMode::Upper | CaseMode::Inverse => true,
                CaseMode::Title | CaseMode::WordCase => index == 0,
                CaseMode::Mixed => self.random.below(MIXED_CASE_ODDS)? == 0,
                CaseMode::Alternating => index % 2 != 0,
                CaseMode::Lower | CaseMode::Blocks => false,
            };
            if upper {
                cased.extend(character.to_uppercase());
            } else {
                cased.push(character);
            }
        }
        if case == CaseMode::Inverse
            && let Some(last) = cased.pop()
        {
            cased.extend(last.to_lowercase());
        }
        Some((cased, letters))
    }

    /// The strength of the password shown, as the C# `UpdateStrengthIndicator`.
    fn update_strength(&mut self, entropy: f64) {
        self.last_entropy_bits = entropy;
        if self.password.is_empty() {
            self.strength = None;
            self.crack_time = None;
            self.issues.clear();
            return;
        }
        let levels = [
            StrengthLevel::Critical,
            StrengthLevel::Weak,
            StrengthLevel::Fair,
            StrengthLevel::Good,
        ];
        let (level, fill) = STRENGTH_BOUNDS
            .iter()
            .zip(levels)
            .find(|((bound, _), _)| entropy < *bound)
            .map_or(
                (StrengthLevel::Strong, STRONG_FILL),
                |((_, fill), level)| (level, *fill),
            );
        self.strength = Some(Strength {
            level,
            fill,
            bits: entropy,
        });
        self.crack_time = rules::crack_time(entropy);
        self.update_issues();
    }

    /// The issues of the password shown, as the C# `UpdateIssuesList`.
    fn update_issues(&mut self) {
        let mut issues = Vec::new();
        let password = self.password.clone();
        if utf16_length(&password) < SHORT_PASSWORD {
            issues.push(Issue::TooShort);
        }
        if self.floor_out_of_reach {
            let ceiling = self.floor_ceiling_bits().floor();
            issues.push(Issue::FloorUnreachable {
                floor: self.entropy_floor_bits(),
                ceiling: whole_bits(ceiling),
            });
        }
        let s = &self.settings;
        if s.mode == GeneratorMode::Leet
            && !s.leet_random_word
            && !sanitize_leet_base_word(&s.leet_base_word).is_empty()
        {
            issues.push(Issue::ChosenWord);
        }
        if s.mode == GeneratorMode::Random {
            if s.include_uppercase && !password.chars().any(char::is_uppercase) {
                issues.push(Issue::NoUpper);
            }
            if s.include_lowercase && !password.chars().any(char::is_lowercase) {
                issues.push(Issue::NoLower);
            }
            if s.include_digits && !password.chars().any(char::is_numeric) {
                issues.push(Issue::NoDigit);
            }
            if s.include_symbols {
                let symbols = self.effective_symbols();
                if !symbols.is_empty() && !password.chars().any(|c| symbols.contains(c)) {
                    issues.push(Issue::NoSpecial);
                }
            }
        }
        self.issues = issues;
    }

    fn update_phonetic(&mut self) {
        self.phonetic = Zeroizing::new(rules::phonetic(&self.password));
    }

    /// No password, nothing about it, as the C# `SetEmptyOutput`.
    fn set_empty_output(&mut self) {
        self.password = Zeroizing::default();
        self.phonetic = Zeroizing::default();
        self.strength = None;
        self.crack_time = None;
        self.issues.clear();
        self.syllable_structure = Zeroizing::default();
        self.syllable_total_length = 0;
    }

    /// `password` first in the history, ten kept, as the C# `AddToHistory`.
    fn add_to_history(&mut self, password: &Zeroizing<String>) {
        if password.is_empty() {
            return;
        }
        self.history.retain(|known| known != password);
        self.history.insert(0, password.clone());
        self.history.truncate(HISTORY_MAX_SIZE);
    }

    // -- The minimum ------------------------------------------------------------------------

    fn capture_before_floor(&self) -> FloorUndo {
        let s = &self.settings;
        FloorUndo {
            length: s.length,
            syllable_length: s.syllable_length,
            syllable_digits: s.syllable_digits,
            syllable_specials: s.syllable_specials,
            passphrase_word_count: s.passphrase_word_count,
            passphrase_digits: s.passphrase_digits,
            passphrase_specials: s.passphrase_specials,
            leet_digits: s.leet_digits,
            leet_specials: s.leet_specials,
        }
    }

    fn restore_before_floor(&mut self, undo: FloorUndo) {
        self.set(Setting::Length(undo.length));
        self.set(Setting::SyllableLength(undo.syllable_length));
        self.set(Setting::SyllableDigits(undo.syllable_digits));
        self.set(Setting::SyllableSpecials(undo.syllable_specials));
        self.set(Setting::PassphraseWordCount(undo.passphrase_word_count));
        self.set(Setting::PassphraseDigits(undo.passphrase_digits));
        self.set(Setting::PassphraseSpecials(undo.passphrase_specials));
        self.set(Setting::LeetDigits(undo.leet_digits));
        self.set(Setting::LeetSpecials(undo.leet_specials));
    }

    /// The settings put where they guarantee the minimum, in one write or none, and undone
    /// when the minimum is lowered, as the C# `ApplyFloorToSettings`.
    fn apply_floor_to_settings(&mut self) {
        let floor = self.entropy_floor_bits();
        let mut sentence = None;
        let mut undo = self.before_floor;
        // Back to the user's own settings before deciding anything.
        if let Some(before) = undo {
            self.suspended = true;
            self.restore_before_floor(before);
            self.suspended = false;
        }
        if floor > 0 {
            let found = self.search_floor(f64::from(floor));
            if found.reached
                && let Some(apply) = found.apply
            {
                undo.get_or_insert_with(|| self.capture_before_floor());
                self.suspended = true;
                sentence = Some(self.apply_found(apply, floor));
                self.suspended = false;
            }
        } else {
            undo = None;
        }
        // After the generation, which clears both.
        self.regenerate_if_ready();
        self.before_floor = undo;
        self.floor_search_notice = sentence;
    }

    /// `apply` written, and what the notice says of it.
    fn apply_found(&mut self, apply: FloorApply, floor: u32) -> FloorSearchNotice {
        match apply {
            FloorApply::Length(length) => {
                self.set(Setting::Length(length));
                FloorSearchNotice::Length { length, floor }
            }
            FloorApply::Syllable(length, digits, specials) => {
                self.set(Setting::SyllableLength(length));
                self.set(Setting::SyllableDigits(digits));
                self.set(Setting::SyllableSpecials(specials));
                FloorSearchNotice::Syllable {
                    length,
                    digits,
                    specials,
                    floor,
                }
            }
            FloorApply::Passphrase(words, digits, specials) => {
                self.set(Setting::PassphraseWordCount(words));
                self.set(Setting::PassphraseDigits(digits));
                self.set(Setting::PassphraseSpecials(specials));
                FloorSearchNotice::Passphrase {
                    words,
                    digits,
                    specials,
                    floor,
                }
            }
            FloorApply::Leet(digits, specials) => {
                self.set(Setting::LeetDigits(digits));
                self.set(Setting::LeetSpecials(specials));
                FloorSearchNotice::Leet {
                    digits,
                    specials,
                    floor,
                }
            }
        }
    }

    /// The search of the mode shown.
    fn search_floor(&self, floor: f64) -> FloorFound {
        match self.settings.mode {
            GeneratorMode::Random => self.search_random_floor(floor),
            GeneratorMode::Syllable => self.search_syllable_floor(floor),
            GeneratorMode::Passphrase => self.search_passphrase_floor(floor),
            GeneratorMode::Leet => self.search_leet_floor(floor),
        }
    }

    /// The most these settings can guarantee, as the C# `FloorCeilingBits`.
    fn floor_ceiling_bits(&self) -> f64 {
        self.search_floor(f64::from(self.entropy_floor_bits()))
            .ceiling
    }

    fn search_random_floor(&self, floor: f64) -> FloorFound {
        let classes = self.charset_classes();
        if classes.is_empty() {
            return FloorFound::NOT_FOUND;
        }
        let length = self.settings.length;
        let mut ceiling: f64 = 0.0;
        for candidate in length.max(1)..=MAXIMUM_LENGTH {
            let bits = rules::guaranteed_random_bits(&classes, candidate);
            ceiling = ceiling.max(bits);
            if bits < floor {
                continue;
            }
            let apply = (candidate != length).then_some(FloorApply::Length(candidate));
            return FloorFound {
                reached: true,
                ceiling,
                apply,
            };
        }
        FloorFound {
            reached: false,
            ceiling,
            apply: None,
        }
    }

    fn search_syllable_floor(&self, floor: f64) -> FloorFound {
        let s = &self.settings;
        let mut ceiling: f64 = 0.0;
        let room = (MAXIMUM_SYLLABLE_EXTRAS.saturating_sub(s.syllable_digits))
            + (MAXIMUM_SYLLABLE_EXTRAS.saturating_sub(s.syllable_specials));
        let mut length = s.syllable_length;
        while length <= MAXIMUM_SYLLABLE_LENGTH {
            for added in 0..=room {
                for more_digits in 0..=added {
                    let digits = s.syllable_digits + more_digits;
                    let specials = s.syllable_specials + (added - more_digits);
                    if digits > MAXIMUM_SYLLABLE_EXTRAS || specials > MAXIMUM_SYLLABLE_EXTRAS {
                        continue;
                    }
                    let bits = self.guaranteed_syllable_bits(length, digits, specials);
                    ceiling = ceiling.max(bits);
                    if bits < floor {
                        continue;
                    }
                    let same = length == s.syllable_length
                        && digits == s.syllable_digits
                        && specials == s.syllable_specials;
                    return FloorFound {
                        reached: true,
                        ceiling,
                        apply: (!same).then_some(FloorApply::Syllable(length, digits, specials)),
                    };
                }
            }
            length += SYLLABLE_LENGTH_STEP;
        }
        FloorFound {
            reached: false,
            ceiling,
            apply: None,
        }
    }

    /// Words before extras: a passphrase is words, as the C# `SearchPassphraseFloor`.
    fn search_passphrase_floor(&self, floor: f64) -> FloorFound {
        let s = &self.settings;
        let mut ceiling: f64 = 0.0;
        let room = (MAXIMUM_LEET_EXTRAS.saturating_sub(s.passphrase_digits))
            + (MAXIMUM_LEET_EXTRAS.saturating_sub(s.passphrase_specials));
        for added in 0..=room {
            for words in s.passphrase_word_count..=MAXIMUM_PASSPHRASE_WORDS {
                for more_digits in 0..=added {
                    let digits = s.passphrase_digits + more_digits;
                    let specials = s.passphrase_specials + (added - more_digits);
                    if digits > MAXIMUM_LEET_EXTRAS || specials > MAXIMUM_LEET_EXTRAS {
                        continue;
                    }
                    let bits = self.guaranteed_passphrase_bits(words, digits, specials);
                    ceiling = ceiling.max(bits);
                    if bits < floor {
                        continue;
                    }
                    let same = words == s.passphrase_word_count
                        && digits == s.passphrase_digits
                        && specials == s.passphrase_specials;
                    return FloorFound {
                        reached: true,
                        ceiling,
                        apply: (!same).then_some(FloorApply::Passphrase(words, digits, specials)),
                    };
                }
            }
        }
        FloorFound {
            reached: false,
            ceiling,
            apply: None,
        }
    }

    fn search_leet_floor(&self, floor: f64) -> FloorFound {
        let s = &self.settings;
        let mut ceiling: f64 = 0.0;
        let room = (MAXIMUM_LEET_EXTRAS.saturating_sub(s.leet_digits))
            + (MAXIMUM_LEET_EXTRAS.saturating_sub(s.leet_specials));
        for added in 0..=room {
            for more_digits in 0..=added {
                let digits = s.leet_digits + more_digits;
                let specials = s.leet_specials + (added - more_digits);
                if digits > MAXIMUM_LEET_EXTRAS || specials > MAXIMUM_LEET_EXTRAS {
                    continue;
                }
                let bits = self.guaranteed_leet_bits(digits, specials);
                ceiling = ceiling.max(bits);
                if bits < floor {
                    continue;
                }
                let same = digits == s.leet_digits && specials == s.leet_specials;
                return FloorFound {
                    reached: true,
                    ceiling,
                    apply: (!same).then_some(FloorApply::Leet(digits, specials)),
                };
            }
        }
        FloorFound {
            reached: false,
            ceiling,
            apply: None,
        }
    }

    /// The blocks kept as many as the units, as the C# `SyncCaseBlocksToUnitCount`.
    fn sync_case_blocks(&mut self) {
        if !self.settings.case_blocks_auto_sync
            || self.current_case_mode() != Some(CaseMode::Blocks)
        {
            return;
        }
        let units = match self.settings.mode {
            GeneratorMode::Syllable => rules::syllable_count(
                self.settings.syllable_length,
                self.settings.syllable_digits,
                self.settings.syllable_specials,
                utf16_length(&self.settings.syllable_separator),
            ),
            GeneratorMode::Passphrase => self.settings.passphrase_word_count,
            GeneratorMode::Random | GeneratorMode::Leet => 0,
        };
        if units == 0 {
            return;
        }
        let wanted = units.clamp(MINIMUM_CASE_BLOCKS, MAXIMUM_CASE_BLOCKS);
        let blocks: Vec<char> = self.settings.case_blocks.chars().collect();
        if wanted == blocks.len() {
            return;
        }
        let resized: String = if wanted < blocks.len() {
            blocks[..wanted].iter().collect()
        } else {
            blocks
                .iter()
                .copied()
                .chain(std::iter::repeat_n(
                    CASE_BLOCK_TOKENS[1],
                    wanted - blocks.len(),
                ))
                .collect()
        };
        self.set(Setting::CaseBlocks(resized));
    }
}

/// `material` put back together with its extras at these positions, as the C# `Rearrange`.
fn rearrange(material: &PlacementMaterial, digits: &[f64], specials: &[f64]) -> Zeroizing<String> {
    let mut chars = material.drawn.clone();
    rules::insert_at_positions(&mut chars, &material.digits, digits);
    rules::insert_at_positions(&mut chars, &material.specials, specials);
    Zeroizing::new(chars.iter().collect())
}

/// The letters of a typed base word, as the C# `SanitizeLeetBaseWord`.
fn sanitize_leet_base_word(word: &str) -> Zeroizing<String> {
    Zeroizing::new(word.chars().filter(|c| c.is_alphabetic()).collect())
}

/// A whole, non-negative number of bits as a count.
fn whole_bits(bits: f64) -> u32 {
    (0..=u32::from(u16::MAX))
        .take_while(|value| f64::from(*value) <= bits)
        .last()
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "password_generator_tests.rs"]
mod tests;
