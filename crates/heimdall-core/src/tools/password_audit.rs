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

//! The Password Audit's engine, as the C# `PasswordAuditView` analyses a password
//! (`PasswordAuditView.xaml.cs:38-556`): offline, against a policy, NIST 800-63B, ANSSI or
//! Custom; the character classes, the entropy of the pool they make, the hundred most common
//! passwords with their leet spellings, keyboard walks, sequences and repeats; a score out of
//! a hundred.
//!
//! Lengths are counted in UTF-16 units and characters compared as UTF-16 units, as the C#
//! `string` counts and compares them.

/// The policies, in the order of the C# policy box (`PolicyKeys`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum AuditPolicy {
    /// NIST 800-63B: eight characters, thirty bits.
    #[default]
    Nist,
    /// ANSSI: twelve characters, every class, fifty bits.
    Anssi,
    /// Custom: eight characters, nothing required.
    Custom,
}

/// What a policy asks, as the C# `PasswordPolicy`.
#[expect(
    clippy::struct_excessive_bools,
    reason = "the C# policy's shape, one requirement per class"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyRules {
    /// The fewest characters.
    pub min_length: usize,
    /// An upper-case letter is required.
    pub require_upper: bool,
    /// A lower-case letter is required.
    pub require_lower: bool,
    /// A digit is required.
    pub require_digit: bool,
    /// A symbol is required.
    pub require_symbol: bool,
    /// The fewest bits; none when zero.
    pub min_entropy: u32,
}

impl AuditPolicy {
    /// Every policy, in the box's order.
    pub const ALL: [Self; 3] = [Self::Nist, Self::Anssi, Self::Custom];

    /// What it asks, as the C# `Policies` table.
    #[must_use]
    pub const fn rules(self) -> PolicyRules {
        match self {
            Self::Nist => PolicyRules {
                min_length: 8,
                require_upper: false,
                require_lower: false,
                require_digit: false,
                require_symbol: false,
                min_entropy: 30,
            },
            Self::Anssi => PolicyRules {
                min_length: 12,
                require_upper: true,
                require_lower: true,
                require_digit: true,
                require_symbol: true,
                min_entropy: 50,
            },
            Self::Custom => PolicyRules {
                min_length: 8,
                require_upper: false,
                require_lower: false,
                require_digit: false,
                require_symbol: false,
                min_entropy: 0,
            },
        }
    }
}

/// The hundred most common passwords, compared whatever their case, as the C#
/// `CommonPasswords`.
const COMMON_PASSWORDS: &[&str] = &[
    "password",
    "123456",
    "12345678",
    "qwerty",
    "abc123",
    "monkey",
    "1234567",
    "letmein",
    "trustno1",
    "dragon",
    "baseball",
    "iloveyou",
    "master",
    "sunshine",
    "ashley",
    "bailey",
    "shadow",
    "123123",
    "654321",
    "superman",
    "qazwsx",
    "michael",
    "football",
    "password1",
    "password123",
    "admin",
    "welcome",
    "charlie",
    "donald",
    "login",
    "princess",
    "qwerty123",
    "solo",
    "passw0rd",
    "starwars",
    "121212",
    "flower",
    "hottie",
    "loveme",
    "zaq1zaq1",
    "hello",
    "monkey123",
    "dragon123",
    "master123",
    "qwerty1",
    "mustang",
    "access",
    "letmein1",
    "batman",
    "111111",
    "000000",
    "1234",
    "12345",
    "123456789",
    "1234567890",
    "password12",
    "iloveu",
    "sunshine1",
    "princess1",
    "football1",
    "charlie1",
    "shadow1",
    "michael1",
    "baseball1",
    "buster",
    "daniel",
    "jessica",
    "pepper",
    "harley",
    "robert",
    "thomas",
    "soccer",
    "hockey",
    "ranger",
    "killer",
    "george",
    "andrew",
    "andrea",
    "joshua",
    "matrix",
    "whatever",
    "cheese",
    "amanda",
    "summer",
    "ginger",
    "cookie",
    "hunter",
    "jennifer",
    "jordan",
    "sparky",
    "abcdef",
    "yankees",
    "dallas",
    "austin",
    "taylor",
    "corvette",
    "merlin",
    "compaq",
    "bigdog",
    "cowboy",
    "camaro",
    "jordan23",
    "london",
    "jasper",
    "apple",
    "brandy",
    "mercedes",
    "thunder",
    "tigers",
    "porsche",
];

/// Keyboard walks and alphabet runs, as the C# `KeyboardPatterns`.
const KEYBOARD_PATTERNS: &[&str] = &[
    "qwerty", "qwertz", "azerty", "qwert", "asdf", "zxcv", "1234", "2345", "3456", "4567", "5678",
    "6789", "7890", "abcd", "bcde", "cdef", "defg", "efgh", "fghi", "ghij", "hijk", "ijkl", "jklm",
    "klmn", "lmno", "mnop", "nopq", "opqr", "pqrs", "qrst", "rstu", "stuv", "tuvw", "uvwx", "vwxy",
    "wxyz",
];

/// The leet spellings undone before a second look at the list, as the C#'s `Replace` chain.
const LEET_UNDONE: &[(char, char)] = &[
    ('@', 'a'),
    ('0', 'o'),
    ('1', 'l'),
    ('3', 'e'),
    ('$', 's'),
    ('!', 'i'),
    ('5', 's'),
    ('7', 't'),
];

/// Characters in a run that makes a sequence or a repeat, as the C# `MinSequenceLength` and
/// `MinRepeatLength`.
const MIN_RUN: usize = 3;

/// The pools a class adds to, as the C# `CalculateEntropy`.
const LOWER_POOL: u32 = 26;
const UPPER_POOL: u32 = 26;
const DIGIT_POOL: u32 = 10;
const SYMBOL_POOL: u32 = 33;

/// The parts of the score, as the C# `CalculateScore`.
const LENGTH_POINTS: f64 = 30.0;
const ENTROPY_POINTS: f64 = 30.0;
const DIVERSITY_POINTS: f64 = 20.0;
const COMMON_PENALTY: f64 = 30.0;
const PATTERN_PENALTY: f64 = 10.0;
const EXCESS_POINTS: f64 = 10.0;
const MISSING_CLASS_PENALTY: f64 = 10.0;
const LENGTH_TARGET_FLOOR: usize = 16;
const ENTROPY_TARGET_FACTOR: f64 = 1.5;
const DEFAULT_ENTROPY_TARGET: f64 = 60.0;
const CLASS_COUNT: f64 = 4.0;
const MAX_SCORE: f64 = 100.0;

/// The score's bounds of each label, as the C#'s `< 25`, `< 50` and `< 75`.
const WEAK_BELOW: u8 = 25;
const FAIR_BELOW: u8 = 50;
const GOOD_BELOW: u8 = 75;

/// A line of the criteria, as the C# criterion labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    /// The length.
    Length,
    /// An upper-case letter.
    Uppercase,
    /// A lower-case letter.
    Lowercase,
    /// A digit.
    Digits,
    /// A symbol.
    Symbols,
    /// The entropy.
    Entropy,
    /// Not a common password.
    Common,
    /// No pattern.
    Patterns,
}

/// A pattern found, as the C# `ToolPwdAuditPattern*` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    /// A keyboard walk or an alphabet run.
    Keyboard,
    /// Three characters ascending or descending.
    Sequence,
    /// Three times the same character.
    Repeat,
}

/// What a criterion's line says on its right, as the C# detail keys.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    /// The length and the policy's minimum.
    Length {
        /// Characters.
        length: usize,
        /// The policy's minimum.
        minimum: usize,
    },
    /// Passed.
    Pass,
    /// Failed, the policy requiring it.
    Fail,
    /// Missing, the policy not requiring it.
    Warn,
    /// The entropy, rounded to a tenth.
    EntropyBits(f64),
    /// Found in the list.
    InCommonList,
    /// Not found in the list.
    NotInCommonList,
    /// The patterns found.
    Patterns(Vec<Pattern>),
}

/// One line of the criteria.
#[derive(Debug, Clone, PartialEq)]
pub struct CriterionResult {
    /// What it is about.
    pub criterion: Criterion,
    /// Whether it passed: its check mark or its cross.
    pub passed: bool,
    /// What it says.
    pub detail: Detail,
}

/// The score's word, as the C# `ToolPwdAuditScore*` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreLabel {
    /// Under 25.
    Weak,
    /// Under 50.
    Fair,
    /// Under 75.
    Good,
    /// 75 and over.
    Strong,
}

/// What the analysis says, as the C# `PasswordAnalysis`.
#[derive(Debug, Clone, PartialEq)]
pub struct PasswordAnalysis {
    /// Out of a hundred.
    pub score: u8,
    /// Its word.
    pub label: ScoreLabel,
    /// The entropy in bits.
    pub entropy: f64,
    /// Found among the common passwords.
    pub is_common: bool,
    /// The patterns found.
    pub patterns: Vec<Pattern>,
    /// The criteria, in the C#'s order.
    pub criteria: Vec<CriterionResult>,
}

/// The classes a password holds, as the C#'s `char.IsUpper`, `IsLower`, `IsDigit` and
/// "not a letter or a digit".
#[derive(Debug, Clone, Copy, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one switch per class of character"
)]
struct Classes {
    upper: bool,
    lower: bool,
    digit: bool,
    symbol: bool,
}

impl Classes {
    fn of(password: &str) -> Self {
        let mut classes = Self::default();
        for character in password.chars() {
            classes.upper |= character.is_uppercase();
            classes.lower |= character.is_lowercase();
            classes.digit |= character.is_numeric();
            classes.symbol |= !character.is_alphanumeric();
        }
        classes
    }

    fn count(self) -> u32 {
        [self.upper, self.lower, self.digit, self.symbol]
            .into_iter()
            .map(u32::from)
            .sum()
    }
}

/// The length of `text` as .NET counts it, in UTF-16 units.
fn utf16_length(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `value` as `f64`, exact for any length a password has.
fn as_f64(value: usize) -> f64 {
    u32::try_from(value).map_or(f64::from(u32::MAX), f64::from)
}

/// The entropy of the pool the password's classes make, times its length, as the C#
/// `CalculateEntropy`: each character counted in one class only, lower before upper before
/// digit, anything else a symbol.
#[must_use]
pub fn entropy(password: &str) -> f64 {
    let (mut lower, mut upper, mut digit, mut symbol) = (false, false, false, false);
    for character in password.chars() {
        if character.is_lowercase() {
            lower = true;
        } else if character.is_uppercase() {
            upper = true;
        } else if character.is_numeric() {
            digit = true;
        } else {
            symbol = true;
        }
    }
    let pool = [
        (lower, LOWER_POOL),
        (upper, UPPER_POOL),
        (digit, DIGIT_POOL),
        (symbol, SYMBOL_POOL),
    ]
    .into_iter()
    .filter(|(found, _)| *found)
    .map(|(_, size)| size)
    .sum::<u32>();
    if pool == 0 {
        return 0.0;
    }
    as_f64(utf16_length(password)) * f64::from(pool).log2()
}

/// Whether `password` is a common one, as typed or once its leet spellings are undone, as
/// the C# `CheckCommonPasswords`.
#[must_use]
pub fn is_common(password: &str) -> bool {
    let listed = |candidate: &str| {
        COMMON_PASSWORDS
            .iter()
            .any(|common| common.eq_ignore_ascii_case(candidate))
    };
    if listed(password) {
        return true;
    }
    let normalized: String = password
        .chars()
        .map(|character| {
            LEET_UNDONE
                .iter()
                .find(|(leet, _)| *leet == character)
                .map_or(character, |(_, plain)| *plain)
        })
        .collect();
    listed(&normalized)
}

/// Whether `units` holds [`MIN_RUN`] units each one more, or each one less, than the one
/// before, as the C# `HasSequentialChars`.
fn has_sequence(units: &[u16]) -> bool {
    let (mut ascending, mut descending) = (1, 1);
    for pair in units.windows(2) {
        let (before, now) = (i32::from(pair[0]), i32::from(pair[1]));
        ascending = if now == before + 1 { ascending + 1 } else { 1 };
        descending = if now == before - 1 { descending + 1 } else { 1 };
        if ascending >= MIN_RUN || descending >= MIN_RUN {
            return true;
        }
    }
    false
}

/// Whether `units` holds the same unit [`MIN_RUN`] times in a row, as the C#
/// `HasRepeatedChars`.
fn has_repeat(units: &[u16]) -> bool {
    let mut run = 1;
    for pair in units.windows(2) {
        run = if pair[0] == pair[1] { run + 1 } else { 1 };
        if run >= MIN_RUN {
            return true;
        }
    }
    false
}

/// The patterns in `password`, as the C# `DetectPatterns`: a keyboard walk, a sequence, a
/// repeat, in that order, each once.
#[must_use]
pub fn patterns(password: &str) -> Vec<Pattern> {
    let lower = password.to_lowercase();
    let units: Vec<u16> = lower.encode_utf16().collect();
    let mut found = Vec::new();
    if KEYBOARD_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        found.push(Pattern::Keyboard);
    }
    if has_sequence(&units) {
        found.push(Pattern::Sequence);
    }
    if has_repeat(&units) {
        found.push(Pattern::Repeat);
    }
    found
}

/// The line of a class: passed when present; failed when the policy requires it and it is
/// missing, a warning when it does not.
fn class_line(criterion: Criterion, present: bool, required: bool) -> CriterionResult {
    let detail = if present {
        Detail::Pass
    } else if required {
        Detail::Fail
    } else {
        Detail::Warn
    };
    CriterionResult {
        criterion,
        passed: present,
        detail,
    }
}

/// The score out of a hundred, as the C# `CalculateScore`, rounded half to even as .NET's
/// `Math.Round`.
fn score(
    length: usize,
    rules: PolicyRules,
    entropy: f64,
    common: bool,
    pattern_count: usize,
    classes: Classes,
) -> u8 {
    let mut score = 0.0;
    let length_ratio =
        (as_f64(length) / as_f64((rules.min_length * 2).max(LENGTH_TARGET_FLOOR))).min(1.0);
    score += length_ratio * LENGTH_POINTS;
    let entropy_target = if rules.min_entropy > 0 {
        f64::from(rules.min_entropy) * ENTROPY_TARGET_FACTOR
    } else {
        DEFAULT_ENTROPY_TARGET
    };
    score += (entropy / entropy_target).min(1.0) * ENTROPY_POINTS;
    score += f64::from(classes.count()) / CLASS_COUNT * DIVERSITY_POINTS;
    if common {
        score -= COMMON_PENALTY;
    }
    score -= as_f64(pattern_count) * PATTERN_PENALTY;
    if length > rules.min_length {
        let excess = (as_f64(length - rules.min_length) / as_f64(rules.min_length)).min(1.0);
        score += excess * EXCESS_POINTS;
    }
    for (required, present) in [
        (rules.require_upper, classes.upper),
        (rules.require_lower, classes.lower),
        (rules.require_digit, classes.digit),
        (rules.require_symbol, classes.symbol),
    ] {
        if required && !present {
            score -= MISSING_CLASS_PENALTY;
        }
    }
    let rounded = score.round_ties_even().clamp(0.0, MAX_SCORE);
    // Clamped to 0..=100 just above, so the conversion is exact.
    (0..=100_u8)
        .find(|value| f64::from(*value) >= rounded)
        .unwrap_or(100)
}

/// The analysis of `password` against `policy`, as the C# `AnalyzePassword`.
#[must_use]
pub fn analyze(password: &str, policy: AuditPolicy) -> PasswordAnalysis {
    let rules = policy.rules();
    let entropy = entropy(password);
    let common = is_common(password);
    let patterns = patterns(password);
    let classes = Classes::of(password);
    let length = utf16_length(password);
    let criteria = vec![
        CriterionResult {
            criterion: Criterion::Length,
            passed: length >= rules.min_length,
            detail: Detail::Length {
                length,
                minimum: rules.min_length,
            },
        },
        class_line(Criterion::Uppercase, classes.upper, rules.require_upper),
        class_line(Criterion::Lowercase, classes.lower, rules.require_lower),
        class_line(Criterion::Digits, classes.digit, rules.require_digit),
        class_line(Criterion::Symbols, classes.symbol, rules.require_symbol),
        CriterionResult {
            criterion: Criterion::Entropy,
            passed: rules.min_entropy == 0 || entropy >= f64::from(rules.min_entropy),
            detail: Detail::EntropyBits((entropy * 10.0).round_ties_even() / 10.0),
        },
        CriterionResult {
            criterion: Criterion::Common,
            passed: !common,
            detail: if common {
                Detail::InCommonList
            } else {
                Detail::NotInCommonList
            },
        },
        if patterns.is_empty() {
            CriterionResult {
                criterion: Criterion::Patterns,
                passed: true,
                detail: Detail::Pass,
            }
        } else {
            CriterionResult {
                criterion: Criterion::Patterns,
                passed: false,
                detail: Detail::Patterns(patterns.clone()),
            }
        },
    ];
    let score = score(length, rules, entropy, common, patterns.len(), classes);
    let label = if score < WEAK_BELOW {
        ScoreLabel::Weak
    } else if score < FAIR_BELOW {
        ScoreLabel::Fair
    } else if score < GOOD_BELOW {
        ScoreLabel::Good
    } else {
        ScoreLabel::Strong
    };
    PasswordAnalysis {
        score,
        label,
        entropy,
        is_common: common,
        patterns,
        criteria,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entropy_is_the_length_times_the_pool_of_the_classes_present() {
        assert!(entropy("").abs() < f64::EPSILON);
        assert!((entropy("abcdefgh") - 8.0 * 26_f64.log2()).abs() < 1e-9);
        assert!((entropy("aB3!") - 4.0 * 95_f64.log2()).abs() < 1e-9);
        assert!((entropy("Password1") - 9.0 * 62_f64.log2()).abs() < 1e-9);
    }

    #[test]
    fn common_passwords_are_found_whatever_their_case_and_their_leet_spelling() {
        assert!(is_common("password"));
        assert!(is_common("PASSWORD"));
        assert!(is_common("p@ssw0rd"));
        assert!(is_common("Dr@g0n"));
        assert!(is_common("5unshine"));
        assert!(!is_common("correct horse battery staple"));
    }

    #[test]
    fn patterns_are_found_in_the_csharp_order() {
        assert_eq!(patterns("qwerty"), [Pattern::Keyboard]);
        assert_eq!(patterns("x1234y"), [Pattern::Keyboard, Pattern::Sequence]);
        assert_eq!(patterns("aaa"), [Pattern::Repeat]);
        assert_eq!(patterns("cba"), [Pattern::Sequence]);
        assert_eq!(patterns("Zq9!mT"), Vec::<Pattern>::new());
        assert_eq!(patterns("ab"), Vec::<Pattern>::new());
    }

    #[test]
    fn a_common_password_scores_weak_under_nist() {
        let analysis = analyze("password", AuditPolicy::Nist);
        // Length 8/16 * 30 = 15, entropy 37.6/45 * 30 = 25.07, one class 5, common -30.
        assert_eq!(analysis.score, 15);
        assert_eq!(analysis.label, ScoreLabel::Weak);
        assert!(analysis.is_common);
        let common = &analysis.criteria[6];
        assert_eq!(common.criterion, Criterion::Common);
        assert!(!common.passed);
        assert_eq!(common.detail, Detail::InCommonList);
        assert_eq!(
            analysis.criteria[1].detail,
            Detail::Warn,
            "NIST requires no capital"
        );
    }

    #[test]
    fn anssi_fails_the_classes_it_requires() {
        let analysis = analyze("abcdefghijkl", AuditPolicy::Anssi);
        assert_eq!(analysis.criteria[1].detail, Detail::Fail);
        assert_eq!(analysis.criteria[3].detail, Detail::Fail);
        assert!(analysis.criteria[5].passed, "56.4 bits pass 50");
        let lengths = &analysis.criteria[0];
        assert!(lengths.passed);
        assert_eq!(
            lengths.detail,
            Detail::Length {
                length: 12,
                minimum: 12
            }
        );
    }

    #[test]
    fn a_strong_password_scores_strong_and_passes_everything() {
        let analysis = analyze("V7#qL9!xR2@wZ5k8$Tm4", AuditPolicy::Anssi);
        assert!(
            analysis.criteria.iter().all(|line| line.passed),
            "{analysis:?}"
        );
        assert_eq!(analysis.label, ScoreLabel::Strong);
        assert_eq!(analysis.criteria[5].detail, Detail::EntropyBits(131.4));
    }

    #[test]
    fn the_score_is_kept_between_nought_and_a_hundred() {
        assert_eq!(analyze("aaa", AuditPolicy::Anssi).score, 0);
        let long = "V7#qL9!xR2@wZ5V7#qL9!xR2@wZ5";
        assert!(analyze(long, AuditPolicy::Custom).score <= 100);
    }
}
