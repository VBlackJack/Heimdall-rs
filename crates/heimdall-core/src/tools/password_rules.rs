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

//! What the Password Generator decides without drawing anything, as the C#
//! `PasswordGeneratorViewModel`'s static parts (`PasswordGeneratorViewModel.cs:100-470`,
//! `1331-1446`, `1861-1936`, `2206-2234`, `2546-2678`, `3278-3454`) and the view's
//! `PlacementBarGeometry`: the character sets, the shapes of a syllable password, the bits
//! each setting guarantees, the case blocks, the positions of the placement bar, the
//! phonetic reading and the crack time.
//!
//! Lengths are counted in UTF-16 units, as the C# `string.Length` counts them.

/// Upper-case letters, as the C# `UppercaseChars`.
pub const UPPERCASE_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Lower-case letters, as the C# `LowercaseChars`.
pub const LOWERCASE_CHARS: &str = "abcdefghijklmnopqrstuvwxyz";

/// Digits, as the C# `DigitChars`.
pub const DIGIT_CHARS: &str = "0123456789";

/// The special characters used unless others are typed, as the C# `DefaultSymbolChars`.
pub const DEFAULT_SYMBOL_CHARS: &str = "!@#$%^&*()-_=+[]{}|;:',.<>?/~`";

/// Every special character a password may hold: printable ASCII punctuation, as the C#
/// `AllowedSpecialChars`.
pub const ALLOWED_SPECIAL_CHARS: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

/// Characters read alike, as the C# `AmbiguousChars`.
pub const AMBIGUOUS_CHARS: &str = "0Oo1lI|";

/// Characters a shell reads as syntax, as the C# `ShellDangerousChars`.
pub const SHELL_DANGEROUS_CHARS: &str = "$^&*'\"\\|`(){}[]<>!~;";

/// Letters that move between AZERTY and QWERTY, as the C# `LayoutUnsafeChars`.
pub const LAYOUT_UNSAFE_CHARS: &str = "aqwzmAQWZM";

/// Consonants of a syllable, as the C# `Consonants`.
pub const CONSONANTS: &[&str] = &[
    "b", "c", "d", "f", "g", "h", "j", "k", "l", "m", "n", "p", "r", "s", "t", "v", "w", "x", "z",
];

/// Vowels of a syllable, as the C# `Vowels`.
pub const VOWELS: &[&str] = &["a", "e", "i", "o", "u", "y"];

/// Consonants of a syllable on any layout, as the C# `LayoutSafeConsonants`.
pub const LAYOUT_SAFE_CONSONANTS: &[&str] = &[
    "b", "c", "d", "f", "g", "h", "j", "k", "l", "n", "p", "r", "s", "t", "v", "x",
];

/// Vowels of a syllable on any layout, as the C# `LayoutSafeVowels`.
pub const LAYOUT_SAFE_VOWELS: &[&str] = &["e", "i", "o", "u", "y"];

/// Consonants closing a syllable, as the C# `EndingConsonants`.
pub const ENDING_CONSONANTS: &[&str] =
    &["b", "d", "f", "g", "k", "l", "m", "n", "p", "r", "s", "t"];

/// What a case block can say: upper, lower, title, as the C# `CaseBlockTokens`.
pub const CASE_BLOCK_TOKENS: [char; 3] = ['U', 'l', 'T'];

/// The pattern a new tool starts with, as the C# `DefaultCaseBlocks`.
pub const DEFAULT_CASE_BLOCKS: &str = "Tl";

/// The fewest and most blocks, as the C# `MinimumCaseBlocks` and `MaximumCaseBlocks`.
pub const MINIMUM_CASE_BLOCKS: usize = 1;
pub const MAXIMUM_CASE_BLOCKS: usize = 10;

/// Characters one syllable block covers, as the C# `CharactersPerSyllableBlock`.
pub const CHARACTERS_PER_SYLLABLE_BLOCK: usize = 2;

/// The room a syllable password keeps for its letters, as the C# `MinimumSyllablePortion`.
pub const MINIMUM_SYLLABLE_PORTION: usize = 2;

/// The ends of the placement bar, in percent, as the C# `MinimumPositionPercent` and
/// `MaximumPositionPercent`.
pub const MINIMUM_POSITION_PERCENT: f64 = 0.0;
pub const MAXIMUM_POSITION_PERCENT: f64 = 100.0;

/// Tenths of a percent a position is kept to, as the C# `PositionPercentDecimals`.
const POSITION_SCALE: f64 = 10.0;

/// What a letter is worth under mixed case, as the C# `MixedCaseBitsPerLetter`.
pub const MIXED_CASE_BITS_PER_LETTER: f64 = 0.81;

/// The guessing rate the crack time is worked out at, as the C# `BruteForceGuessesPerSecond`.
pub const BRUTE_FORCE_GUESSES_PER_SECOND: f64 = 10_000_000_000.0;

/// Characters the phonetic reading spells at most, as the C# `PhoneticMaxLength`.
pub const PHONETIC_MAX_LENGTH: usize = 32;

/// What separates the words of the phonetic reading, as the C# `" - "`.
const PHONETIC_SEPARATOR: &str = " - ";

/// The leet table, as the C# `LeetSubstitutions`.
pub const LEET_SUBSTITUTIONS: &[(char, char)] = &[
    ('a', '@'),
    ('b', '8'),
    ('e', '3'),
    ('g', '9'),
    ('i', '1'),
    ('l', '!'),
    ('o', '0'),
    ('s', '5'),
    ('t', '7'),
];

/// The NATO alphabet and the digits' names, as the C# `NatoAlphabet`.
const NATO_ALPHABET: &[(char, &str)] = &[
    ('A', "Alpha"),
    ('B', "Bravo"),
    ('C', "Charlie"),
    ('D', "Delta"),
    ('E', "Echo"),
    ('F', "Foxtrot"),
    ('G', "Golf"),
    ('H', "Hotel"),
    ('I', "India"),
    ('J', "Juliet"),
    ('K', "Kilo"),
    ('L', "Lima"),
    ('M', "Mike"),
    ('N', "November"),
    ('O', "Oscar"),
    ('P', "Papa"),
    ('Q', "Quebec"),
    ('R', "Romeo"),
    ('S', "Sierra"),
    ('T', "Tango"),
    ('U', "Uniform"),
    ('V', "Victor"),
    ('W', "Whiskey"),
    ('X', "X-ray"),
    ('Y', "Yankee"),
    ('Z', "Zulu"),
    ('0', "Zero"),
    ('1', "One"),
    ('2', "Two"),
    ('3', "Three"),
    ('4', "Four"),
    ('5', "Five"),
    ('6', "Six"),
    ('7', "Seven"),
    ('8', "Eight"),
    ('9', "Nine"),
];

/// The special characters' names, as the C# `SpecialCharNames`.
const SPECIAL_CHAR_NAMES: &[(char, &str)] = &[
    ('!', "Exclamation"),
    ('@', "At"),
    ('#', "Hash"),
    ('$', "Dollar"),
    ('%', "Percent"),
    ('^', "Caret"),
    ('&', "Ampersand"),
    ('*', "Asterisk"),
    ('(', "OpenParen"),
    (')', "CloseParen"),
    ('-', "Dash"),
    ('_', "Underscore"),
    ('=', "Equals"),
    ('+', "Plus"),
    ('[', "OpenBracket"),
    (']', "CloseBracket"),
    ('{', "OpenBrace"),
    ('}', "CloseBrace"),
    ('\\', "Backslash"),
    ('/', "Slash"),
    (';', "Semicolon"),
    (':', "Colon"),
    ('\'', "Apostrophe"),
    ('"', "Quote"),
    (',', "Comma"),
    ('.', "Period"),
    ('<', "LessThan"),
    ('>', "GreaterThan"),
    ('?', "Question"),
    ('~', "Tilde"),
    ('`', "Backtick"),
    ('|', "Pipe"),
    (' ', "Space"),
];

/// How a word is cased, as the C# `SyllableCase`: its index is kept in presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum CaseMode {
    /// One letter in four upper-cased at random.
    #[default]
    Mixed,
    /// Lower case.
    Lower,
    /// Upper case.
    Upper,
    /// The first letter upper-cased.
    Title,
    /// Every other letter upper-cased.
    Alternating,
    /// Each unit's first letter upper-cased.
    WordCase,
    /// Upper case but the last letter.
    Inverse,
    /// Each unit cased by its block of the pattern.
    Blocks,
}

impl CaseMode {
    /// Every mode, in the box's order.
    pub const ALL: [Self; 8] = [
        Self::Mixed,
        Self::Lower,
        Self::Upper,
        Self::Title,
        Self::Alternating,
        Self::WordCase,
        Self::Inverse,
        Self::Blocks,
    ];

    /// The mode at `index`, held to the table, as the C# `CaseModeAt`.
    #[must_use]
    pub fn at(index: i32) -> Self {
        let last = Self::ALL.len() - 1;
        Self::ALL[usize::try_from(index).unwrap_or(0).min(last)]
    }

    /// Its index, as presets keep it.
    #[must_use]
    pub fn index(self) -> i32 {
        Self::ALL
            .iter()
            .position(|mode| *mode == self)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(0)
    }
}

/// Where the digits and specials go, as the C# `Placement`: its index is kept in presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Placement {
    /// Each at a random place.
    #[default]
    Random,
    /// At the start.
    Start,
    /// At the end.
    End,
    /// In the middle.
    Middle,
    /// Each at its cursor of the placement bar.
    Positions,
}

impl Placement {
    /// Every placement, in the box's order.
    pub const ALL: [Self; 5] = [
        Self::Random,
        Self::Start,
        Self::End,
        Self::Middle,
        Self::Positions,
    ];

    /// The placement at `index`, held to the table, as the C# `PlacementAt`.
    #[must_use]
    pub fn at(index: i32) -> Self {
        let last = Self::ALL.len() - 1;
        Self::ALL[usize::try_from(index).unwrap_or(0).min(last)]
    }

    /// Its index, as presets keep it.
    #[must_use]
    pub fn index(self) -> i32 {
        Self::ALL
            .iter()
            .position(|placement| *placement == self)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(0)
    }
}

/// The length of `text` as .NET counts it, in UTF-16 units.
#[must_use]
pub fn utf16_length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `value` as `f64`, exact for any count a password has.
#[must_use]
pub fn as_f64(value: usize) -> f64 {
    u32::try_from(value).map_or(f64::from(u32::MAX), f64::from)
}

/// The characters of `typed` a password may hold, each once, in their order, as the C#
/// `SanitizeCustomSpecials`.
#[must_use]
pub fn sanitize_custom_specials(typed: &str) -> String {
    let mut kept = String::new();
    for character in typed.chars() {
        if ALLOWED_SPECIAL_CHARS.contains(character) && !kept.contains(character) {
            kept.push(character);
        }
    }
    kept
}

/// What the specials box is worth to the generator, as the C# `GetEffectiveSymbols`: the
/// typed ones when there are any, held to punctuation, the shell's syntax taken out when
/// `cli_safe`. In the random mode the typed ones count only while the symbols box is ticked.
#[must_use]
pub fn effective_symbols(
    custom: &str,
    random_mode: bool,
    include_symbols: bool,
    cli_safe: bool,
) -> String {
    let typed = !custom.trim().is_empty();
    let use_custom = if random_mode {
        include_symbols && typed
    } else {
        typed
    };
    let symbols = if use_custom {
        sanitize_custom_specials(custom)
    } else {
        DEFAULT_SYMBOL_CHARS.to_owned()
    };
    if cli_safe {
        symbols
            .chars()
            .filter(|character| !SHELL_DANGEROUS_CHARS.contains(*character))
            .collect()
    } else {
        symbols
    }
}

/// The classes the random mode draws from, each once its exclusions have run, an emptied
/// class left out, as the C# `BuildCharsetClasses`.
#[must_use]
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "the C# tool's boxes, one per class and exclusion"
)]
pub fn charset_classes(
    upper: bool,
    lower: bool,
    digits: bool,
    symbols: Option<&str>,
    exclude_ambiguous: bool,
    layout_safe: bool,
) -> Vec<Vec<char>> {
    let take = |characters: &str| -> Option<Vec<char>> {
        let kept: Vec<char> = characters
            .chars()
            .filter(|character| !(exclude_ambiguous && AMBIGUOUS_CHARS.contains(*character)))
            .filter(|character| !(layout_safe && LAYOUT_UNSAFE_CHARS.contains(*character)))
            .collect();
        (!kept.is_empty()).then_some(kept)
    };
    [
        upper.then_some(UPPERCASE_CHARS),
        lower.then_some(LOWERCASE_CHARS),
        digits.then_some(DIGIT_CHARS),
        symbols,
    ]
    .into_iter()
    .flatten()
    .filter_map(take)
    .collect()
}

/// The share of unconstrained draws carrying every class, by inclusion and exclusion, as
/// the C# `ValidDrawShare`.
#[must_use]
pub fn valid_draw_share(classes: &[Vec<char>], length: usize) -> f64 {
    if classes.is_empty() || length == 0 || classes.len() > length {
        return 0.0;
    }
    let total: usize = classes.iter().map(Vec::len).sum();
    if total == 0 {
        return 0.0;
    }
    let exponent = i32::try_from(length).unwrap_or(i32::MAX);
    let mut share = 0.0;
    for subset in 0..(1_usize << classes.len()) {
        let (mut avoided, mut avoided_classes) = (0, 0);
        for (index, class) in classes.iter().enumerate() {
            if subset & (1 << index) != 0 {
                avoided += class.len();
                avoided_classes += 1;
            }
        }
        let term = (as_f64(total - avoided) / as_f64(total)).powi(exponent);
        share += if avoided_classes % 2 == 0 {
            term
        } else {
            -term
        };
    }
    share
}

/// The bits a random password of `length` carries with every class promised, as the C#
/// `GuaranteedRandomBits`.
#[must_use]
pub fn guaranteed_random_bits(classes: &[Vec<char>], length: usize) -> f64 {
    let total: usize = classes.iter().map(Vec::len).sum();
    if total == 0 || length == 0 {
        return 0.0;
    }
    let unconstrained = as_f64(total).log2() * as_f64(length);
    let share = valid_draw_share(classes, length);
    if share <= 0.0 {
        unconstrained
    } else {
        unconstrained + share.log2()
    }
}

/// Whether `password` carries a character of every class, as the C# `CarriesEveryClass`.
#[must_use]
pub fn carries_every_class(password: &[char], classes: &[Vec<char>]) -> bool {
    classes
        .iter()
        .all(|class| password.iter().any(|character| class.contains(character)))
}

/// A syllable password's shape at `total` characters: the syllables' portion and the digits
/// and specials it has room for, the larger count cut first, as the C# `ResolveSyllableShape`.
#[must_use]
pub fn syllable_shape(
    total: usize,
    wanted_digits: usize,
    wanted_specials: usize,
) -> (usize, usize, usize) {
    let (mut digits, mut specials) = (wanted_digits, wanted_specials);
    while total.saturating_sub(digits + specials) < MINIMUM_SYLLABLE_PORTION
        && digits + specials > 0
    {
        if specials >= digits {
            specials -= 1;
        } else {
            digits -= 1;
        }
    }
    (total.saturating_sub(digits + specials), digits, specials)
}

/// The syllables a password of this shape holds, each after the first paying for its
/// separator, as the C# `SyllableCountFor`.
#[must_use]
pub fn syllable_count(
    total: usize,
    wanted_digits: usize,
    wanted_specials: usize,
    separator_length: usize,
) -> usize {
    let (portion, _, _) = syllable_shape(total, wanted_digits, wanted_specials);
    (portion + separator_length) / (CHARACTERS_PER_SYLLABLE_BLOCK + separator_length)
}

/// The consonants and vowels of a syllable, as the C# picks them by `LayoutSafe`.
#[must_use]
pub fn syllable_letters(layout_safe: bool) -> (&'static [&'static str], &'static [&'static str]) {
    if layout_safe {
        (LAYOUT_SAFE_CONSONANTS, LAYOUT_SAFE_VOWELS)
    } else {
        (CONSONANTS, VOWELS)
    }
}

/// The consonants closing a syllable, those that move between layouts left out when
/// `layout_safe`, as the C# `endings`.
#[must_use]
pub fn ending_consonants(layout_safe: bool) -> Vec<&'static str> {
    ENDING_CONSONANTS
        .iter()
        .copied()
        .filter(|ending| {
            !layout_safe
                || !ending
                    .chars()
                    .next()
                    .is_some_and(|first| LAYOUT_UNSAFE_CHARS.contains(first))
        })
        .collect()
}

/// The bits `digits` digits and `specials` specials drawn from `symbol_count` carry, as the
/// C# `ExtrasBits`.
#[must_use]
pub fn extras_bits(digits: usize, specials: usize, symbol_count: usize) -> f64 {
    let mut bits = if digits > 0 {
        as_f64(DIGIT_CHARS.len()).log2() * as_f64(digits)
    } else {
        0.0
    };
    if specials > 0 && symbol_count > 0 {
        bits += as_f64(symbol_count).log2() * as_f64(specials);
    }
    bits
}

/// A case pattern held to what the editor makes: one to ten of the three tokens, the default
/// when none is left, as the C# `SanitizeCaseBlocks`.
#[must_use]
pub fn sanitize_case_blocks(pattern: &str) -> String {
    let kept: String = pattern
        .chars()
        .filter(|character| CASE_BLOCK_TOKENS.contains(character))
        .take(MAXIMUM_CASE_BLOCKS)
        .collect();
    if kept.is_empty() {
        DEFAULT_CASE_BLOCKS.to_owned()
    } else {
        kept
    }
}

/// `unit` cased by its block, the `unit_index`th of `pattern` repeated, as the C#
/// `ApplyCaseBlock`.
#[must_use]
pub fn apply_case_block(unit: &str, unit_index: usize, pattern: &str) -> String {
    let pattern: Vec<char> = sanitize_case_blocks(pattern).chars().collect();
    match pattern[unit_index % pattern.len()] {
        'U' => unit.to_uppercase(),
        'T' => title_case(unit),
        _ => unit.to_lowercase(),
    }
}

/// `word`'s first character upper-cased, the others lower-cased.
#[must_use]
pub fn title_case(word: &str) -> String {
    let mut characters = word.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + &characters.as_str().to_lowercase()
    })
}

/// `percent` held to the bar and kept to a tenth, half a tenth away from zero, as the C#
/// `ClampPercent`.
#[must_use]
pub fn clamp_percent(percent: f64) -> f64 {
    let clamped = if percent.is_nan() {
        MINIMUM_POSITION_PERCENT
    } else {
        percent.clamp(MINIMUM_POSITION_PERCENT, MAXIMUM_POSITION_PERCENT)
    };
    (clamped * POSITION_SCALE).round() / POSITION_SCALE
}

/// `count` positions, each in the middle of its share of the bar, as the C#
/// `DistributeEvenly`.
#[must_use]
pub fn distribute_evenly(count: usize) -> Vec<f64> {
    (0..count)
        .map(|index| {
            clamp_percent((as_f64(index) + 0.5) / as_f64(count) * MAXIMUM_POSITION_PERCENT)
        })
        .collect()
}

/// The positions written in `text`, comma separated, the unreadable ones left out, each
/// held to the bar, as the C# `ParsePositions`.
#[must_use]
pub fn parse_positions(text: &str) -> Vec<f64> {
    text.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| entry.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .map(clamp_percent)
        .collect()
}

/// `positions` written as presets keep them, as the C# `FormatPositions`.
#[must_use]
pub fn format_positions(positions: &[f64]) -> String {
    positions
        .iter()
        .map(|value| format_percent(clamp_percent(*value)))
        .collect::<Vec<_>>()
        .join(",")
}

/// A percent as .NET's invariant culture writes a double: no trailing `.0`.
#[must_use]
pub fn format_percent(value: f64) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_owned)
}

/// `current` brought to `count` positions: cut when longer, the new ones spread evenly when
/// shorter, as the C# `ResizePositions`.
#[must_use]
pub fn resize_positions(current: &[f64], count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    if current.len() >= count {
        return current[..count].to_vec();
    }
    let mut spread = distribute_evenly(count);
    spread[..current.len()].copy_from_slice(current);
    spread
}

/// Each of `extras` put at its percent of `chars` as it stands when the insertion starts,
/// as the C# `InsertAtPositions`.
pub fn insert_at_positions(chars: &mut Vec<char>, extras: &[char], percents: &[f64]) {
    if extras.is_empty() {
        return;
    }
    let length = as_f64(chars.len());
    let mut placed: Vec<(char, usize, usize)> = extras
        .iter()
        .enumerate()
        .map(|(index, character)| {
            let percent = percents
                .get(index)
                .copied()
                .unwrap_or(MAXIMUM_POSITION_PERCENT);
            let at = (percent / MAXIMUM_POSITION_PERCENT * length).round();
            (*character, round_to_index(at), index)
        })
        .collect();
    placed.sort_by_key(|(_, at, order)| (*at, *order));
    for (offset, (character, at, _)) in placed.into_iter().enumerate() {
        let at = (at + offset).min(chars.len());
        chars.insert(at, character);
    }
}

/// A whole `value` as an index, nought when it is not positive.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a place in a password, far below usize::MAX and never negative"
)]
fn round_to_index(value: f64) -> usize {
    if value <= 0.0 { 0 } else { value as usize }
}

/// The phonetic reading of `password`, as the C# `UpdatePhoneticDisplay`: a letter by its
/// NATO word in its case, a digit and a special by its character and its name; empty past
/// [`PHONETIC_MAX_LENGTH`].
#[must_use]
pub fn phonetic(password: &str) -> String {
    if password.is_empty() || utf16_length(password) > PHONETIC_MAX_LENGTH {
        return String::new();
    }
    let name_of = |table: &[(char, &'static str)], key: char| {
        table
            .iter()
            .find(|(known, _)| *known == key)
            .map(|(_, name)| *name)
    };
    let parts: Vec<String> = password
        .chars()
        .map(|character| {
            if character.is_alphabetic() {
                let key = character.to_ascii_uppercase();
                match name_of(NATO_ALPHABET, key) {
                    Some(word) if character.is_uppercase() => word.to_uppercase(),
                    Some(word) => word.to_lowercase(),
                    None => character.to_string(),
                }
            } else if character.is_numeric() {
                name_of(NATO_ALPHABET, character).map_or_else(
                    || character.to_string(),
                    |word| format!("{character}:{word}"),
                )
            } else {
                name_of(SPECIAL_CHAR_NAMES, character).map_or_else(
                    || character.to_string(),
                    |name| format!("{character}:{name}"),
                )
            }
        })
        .collect();
    parts.join(PHONETIC_SEPARATOR)
}

/// How long a password takes to crack, as the C# `UpdateCrackTimeEstimate` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrackTime {
    /// Under a second.
    Instant,
    /// Seconds.
    Seconds(u64),
    /// Minutes.
    Minutes(u64),
    /// Hours.
    Hours(u64),
    /// Days.
    Days(u64),
    /// Years.
    Years(u64),
    /// Centuries.
    Centuries(u64),
    /// A million years or more.
    Forever,
}

/// Seconds in a minute, an hour, a day and a year of 365.25 days.
const MINUTE: f64 = 60.0;
const HOUR: f64 = 3600.0;
const DAY: f64 = 86_400.0;
const YEAR: f64 = 365.25 * DAY;
const CENTURY_YEARS: f64 = 100.0;
const FOREVER_YEARS: f64 = 1_000_000.0;
const ENTROPY_CAP: f64 = 256.0;

/// The whole part of `value`, as the C#'s `(int)` cast: every value here is under ten
/// thousand.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a count of units under ten thousand, never negative"
)]
fn whole(value: f64) -> u64 {
    value as u64
}

/// The average time to crack `entropy` bits at the stated rate, as the C#
/// `UpdateCrackTimeEstimate`; `None` for no entropy.
#[must_use]
pub fn crack_time(entropy: f64) -> Option<CrackTime> {
    if entropy <= 0.0 {
        return None;
    }
    let seconds = entropy.min(ENTROPY_CAP).exp2() / (2.0 * BRUTE_FORCE_GUESSES_PER_SECOND);
    Some(if seconds < 1.0 {
        CrackTime::Instant
    } else if seconds < MINUTE {
        CrackTime::Seconds(whole(seconds))
    } else if seconds < HOUR {
        CrackTime::Minutes(whole(seconds / MINUTE))
    } else if seconds < DAY {
        CrackTime::Hours(whole(seconds / HOUR))
    } else if seconds < YEAR {
        CrackTime::Days(whole(seconds / DAY))
    } else if seconds < CENTURY_YEARS * YEAR {
        CrackTime::Years(whole(seconds / YEAR))
    } else if seconds < FOREVER_YEARS * YEAR {
        CrackTime::Centuries(whole(seconds / (CENTURY_YEARS * YEAR)))
    } else {
        CrackTime::Forever
    })
}

/// The guessing rate as a power of ten, as the C# `GuessRateText`.
#[must_use]
pub fn guess_rate_text() -> String {
    let exponent = BRUTE_FORCE_GUESSES_PER_SECOND.log10().floor();
    let mantissa = BRUTE_FORCE_GUESSES_PER_SECOND / 10_f64.powf(exponent);
    if (mantissa - 1.0).abs() < 0.001 {
        format!("10^{exponent}")
    } else {
        format!("{} x 10^{exponent}", format_percent(mantissa))
    }
}

/// The places on a track of the placement bar, one more than the characters it is read
/// against, as the C# `PlacementBarGeometry.SlotCount`.
#[must_use]
pub fn slot_count(
    password_length: usize,
    digit_count: usize,
    special_count: usize,
    digits: bool,
) -> usize {
    let Some(drawn) = password_length
        .checked_sub(digit_count + special_count)
        .filter(|drawn| *drawn >= 1)
    else {
        return 0;
    };
    (if digits { drawn } else { drawn + digit_count }) + 1
}

/// The share of the bar one place is worth, as the C# `StepPercent`.
#[must_use]
pub fn step_percent(slots: usize) -> f64 {
    if slots < 2 {
        0.0
    } else {
        MAXIMUM_POSITION_PERCENT / as_f64(slots - 1)
    }
}

/// The place nearest `percent`, as the C# `SnapToSlot`.
#[must_use]
pub fn snap_to_slot(slots: usize, percent: f64) -> f64 {
    if slots < 2 {
        return percent;
    }
    let step = step_percent(slots);
    ((percent / step).round_ties_even() * step)
        .clamp(MINIMUM_POSITION_PERCENT, MAXIMUM_POSITION_PERCENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 0.005
    }

    #[test]
    fn specials_are_held_to_ascii_punctuation() {
        // As the C# Specials_AreHeldToAsciiPunctuation.
        for (typed, expected) in [
            ("", ""),
            ("!@#", "!@#"),
            ("!!!@@@", "!@"),
            ("abc123", ""),
            ("a!b@c#", "!@#"),
            ("\u{20ac}\u{a3}\u{a5}", ""),
            ("!\u{20ac}@", "!@"),
            ("! @ #", "!@#"),
        ] {
            assert_eq!(sanitize_custom_specials(typed), expected, "{typed:?}");
        }
        assert_eq!(
            sanitize_custom_specials(&ALLOWED_SPECIAL_CHARS.repeat(3)),
            ALLOWED_SPECIAL_CHARS
        );
    }

    #[test]
    fn the_strength_figure_pays_for_the_promise() {
        // As the C# TheStrengthFigure_PaysForThePromise: four classes, 94 characters.
        let classes = charset_classes(true, true, true, Some(DEFAULT_SYMBOL_CHARS), false, false);
        for (length, expected) in [(8, 51.10), (12, 77.79), (16, 104.11), (24, 156.47)] {
            let bits = guaranteed_random_bits(&classes, length);
            assert!(close(bits, expected), "{length}: {bits}");
        }
        assert!(valid_draw_share(&classes, 3).abs() < f64::EPSILON);
    }

    #[test]
    fn exclusions_cut_into_the_classes_and_an_emptied_one_is_left_out() {
        let classes = charset_classes(true, false, true, None, true, true);
        assert_eq!(classes.len(), 2);
        assert!(!classes[0].contains(&'O') && !classes[0].contains(&'A'));
        assert!(!classes[1].contains(&'0') && !classes[1].contains(&'1'));
        assert!(charset_classes(false, false, false, Some(""), false, false).is_empty());
    }

    #[test]
    fn the_symbols_used_follow_the_box_and_the_cli_switch() {
        assert_eq!(
            effective_symbols("", true, true, false),
            DEFAULT_SYMBOL_CHARS
        );
        assert_eq!(effective_symbols("!@x", true, true, false), "!@");
        assert_eq!(
            effective_symbols("!@x", true, false, false),
            DEFAULT_SYMBOL_CHARS
        );
        assert_eq!(effective_symbols("!@", false, false, false), "!@");
        assert_eq!(effective_symbols("!@#$", false, false, true), "@#");
    }

    #[test]
    fn syllable_counts_give_way_to_the_length() {
        // As the C# SyllableCounts_GiveWayToTheLength_AndSayThatTheyDid.
        assert_eq!(syllable_shape(8, 6, 6), (2, 3, 3));
        assert_eq!(syllable_shape(8, 1, 6), (2, 1, 5));
        assert_eq!(syllable_shape(8, 6, 1), (2, 5, 1));
        assert_eq!(syllable_shape(8, 4, 4), (2, 3, 3));
        assert_eq!(syllable_count(16, 2, 1, 0), 6);
        assert_eq!(syllable_count(12, 0, 0, 1), 4);
    }

    #[test]
    fn case_patterns_are_held_to_what_the_editor_makes() {
        // As the C# CaseBlocks_ReadFromAPresetAreHeldToWhatTheEditorCanProduce.
        for (stored, expected) in [
            ("", "Tl"),
            ("xyz", "Tl"),
            ("U l T", "UlT"),
            ("UUUUUUUUUUUUUUU", "UUUUUUUUUU"),
        ] {
            assert_eq!(sanitize_case_blocks(stored), expected);
        }
        assert_eq!(apply_case_block("hello", 0, "UlT"), "HELLO");
        assert_eq!(apply_case_block("HELLO", 1, "UlT"), "hello");
        assert_eq!(apply_case_block("hELLO", 2, "UlT"), "Hello");
        assert_eq!(
            apply_case_block("abc", 3, "UlT"),
            "ABC",
            "the pattern repeats"
        );
    }

    #[test]
    fn positions_spread_evenly_and_read_back_held_to_the_bar() {
        // As the C# Positions_SpreadEvenlyAroundTheMiddleOfEachShare and
        // Positions_ReadFromAPresetAreHeldToTheBar.
        assert_eq!(distribute_evenly(1), [50.0]);
        assert_eq!(distribute_evenly(2), [25.0, 75.0]);
        assert_eq!(distribute_evenly(4), [12.5, 37.5, 62.5, 87.5]);
        assert!(parse_positions("").is_empty());
        assert!(parse_positions("nonsense").is_empty());
        assert_eq!(parse_positions(" 10 , 20 "), [10.0, 20.0]);
        assert_eq!(parse_positions("-5,250"), [0.0, 100.0]);
        assert_eq!(parse_positions("33.333"), [33.3]);
        assert_eq!(format_positions(&[12.5, 50.0, 100.0]), "12.5,50,100");
        assert_eq!(resize_positions(&[10.0], 3), [10.0, 50.0, 83.3]);
        assert_eq!(resize_positions(&[10.0, 20.0, 30.0], 2), [10.0, 20.0]);
    }

    #[test]
    fn extras_go_where_their_cursors_are() {
        let mut chars: Vec<char> = "abcdefgh".chars().collect();
        insert_at_positions(&mut chars, &['1', '2'], &[0.0, 100.0]);
        assert_eq!(chars.iter().collect::<String>(), "1abcdefgh2");
        let mut chars: Vec<char> = "abcdefgh".chars().collect();
        insert_at_positions(&mut chars, &['1'], &[50.0]);
        assert_eq!(chars.iter().collect::<String>(), "abcd1efgh");
        let mut chars: Vec<char> = "abcd".chars().collect();
        insert_at_positions(&mut chars, &['1', '2'], &[50.0, 50.0]);
        assert_eq!(chars.iter().collect::<String>(), "ab12cd", "side by side");
    }

    #[test]
    fn the_phonetic_reading_spells_each_character() {
        assert_eq!(phonetic("aB3!"), "alpha - BRAVO - 3:Three - !:Exclamation");
        assert_eq!(phonetic(&"a".repeat(33)), "");
        assert_eq!(phonetic(""), "");
    }

    #[test]
    fn the_crack_time_takes_its_unit_and_the_rate_is_spelled_from_its_constant() {
        assert_eq!(crack_time(0.0), None);
        assert_eq!(crack_time(20.0), Some(CrackTime::Instant));
        // 2^40 / 2e10 = 54.97 seconds.
        assert_eq!(crack_time(40.0), Some(CrackTime::Seconds(54)));
        assert_eq!(crack_time(50.0), Some(CrackTime::Hours(15)));
        assert_eq!(crack_time(60.0), Some(CrackTime::Years(1)));
        assert_eq!(crack_time(70.0), Some(CrackTime::Centuries(18)));
        assert_eq!(crack_time(80.0), Some(CrackTime::Forever));
        assert_eq!(crack_time(128.0), Some(CrackTime::Forever));
        assert_eq!(guess_rate_text(), "10^10");
    }

    #[test]
    fn the_placement_bar_has_one_place_per_character_and_one_more() {
        // As the C# PlacementBarGeometryTests.
        assert_eq!(slot_count(20, 2, 2, true), 17);
        assert_eq!(slot_count(20, 2, 2, false), 19);
        assert_eq!(slot_count(30, 2, 2, true), 27);
        assert_eq!(slot_count(4, 2, 2, true), 0);
        assert!(close(step_percent(17), 6.25));
        assert!(step_percent(1).abs() < f64::EPSILON);
        assert!(close(snap_to_slot(17, 7.0), 6.25));
        assert!(close(snap_to_slot(17, 11.0), 12.5));
        assert!(close(snap_to_slot(17, 120.0), 100.0));
        assert!(close(snap_to_slot(17, -5.0), 0.0));
    }

    #[test]
    fn indices_of_cases_and_placements_are_held_to_their_tables() {
        assert_eq!(CaseMode::at(-1), CaseMode::Mixed);
        assert_eq!(CaseMode::at(7), CaseMode::Blocks);
        assert_eq!(CaseMode::at(99), CaseMode::Blocks);
        assert_eq!(CaseMode::WordCase.index(), 5);
        assert_eq!(Placement::at(4), Placement::Positions);
        assert_eq!(Placement::at(9), Placement::Positions);
        assert_eq!(Placement::Middle.index(), 3);
    }
}
