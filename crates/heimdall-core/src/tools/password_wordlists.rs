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

//! The passphrase word lists, as the C# `PassphraseLanguages` table and `LoadWordListFile`
//! (`PasswordGeneratorViewModel.cs:389-469`, `3534-3602`): English, French, Spanish and
//! Latin, appended to and never reordered since their index is kept in presets.
//!
//! The lists are the C#'s `Assets/wordlist_*.txt`, from the same author's genpwd-pro under
//! the same licence, built into the program: each word trimmed and lower-cased, kept at three
//! to twelve characters, each once; a list of fewer than fifty words falls back to the C#'s
//! fifty. Their first lines, a licence header, start with `#` and are skipped.

use std::sync::LazyLock;

/// Shortest and longest word kept, as the C# loader's `3` and `12`.
const MIN_WORD_LENGTH: usize = 3;
const MAX_WORD_LENGTH: usize = 12;

/// Fewest words a list must keep not to fall back, as the C#'s `50`.
const MIN_WORDS: usize = 50;

/// What starts a line of the header.
const COMMENT_MARK: char = '#';

/// The C#'s English fallback, `FallbackEnglishWords`.
const FALLBACK_ENGLISH: &[&str] = &[
    "anchor", "apple", "arrow", "badge", "beach", "bridge", "cabin", "candle", "castle", "cherry",
    "circle", "cloud", "coffee", "copper", "coral", "crane", "crystal", "delta", "desert",
    "dolphin", "dragon", "eagle", "ember", "falcon", "flame", "forest", "garden", "glacier",
    "golden", "hammer", "harbor", "helmet", "honey", "hunter", "island", "jacket", "jewel",
    "jungle", "ladder", "lantern", "marble", "meadow", "mirror", "monkey", "mountain", "nature",
    "noble", "ocean", "oracle", "palace",
];

/// The C#'s French fallback, `FallbackFrenchWords`.
const FALLBACK_FRENCH: &[&str] = &[
    "abricot", "amande", "ancre", "aurore", "balcon", "barque", "bonnet", "bougie", "branche",
    "cabane", "canard", "cerise", "chalet", "chemin", "cheval", "citron", "coffre", "comete",
    "cristal", "dauphin", "desert", "dragon", "enigme", "etoile", "faucon", "flamme", "fleuve",
    "fortin", "galion", "glacier", "harpon", "jardin", "jasmin", "jungle", "lanterne", "marbre",
    "miroir", "montagne", "moulin", "nature", "oiseau", "olive", "orange", "palmier", "pensee",
    "portail", "radeau", "renard", "soleil", "volcan",
];

/// The C#'s Spanish fallback, `FallbackSpanishWords`.
const FALLBACK_SPANISH: &[&str] = &[
    "abeja",
    "acero",
    "aguja",
    "aldea",
    "ancla",
    "anillo",
    "arcilla",
    "ardilla",
    "arena",
    "arroyo",
    "ballena",
    "bandera",
    "barco",
    "bodega",
    "bosque",
    "brisa",
    "bronce",
    "caballo",
    "cadena",
    "calabaza",
    "camino",
    "campana",
    "cantera",
    "caracol",
    "cascada",
    "castillo",
    "cereza",
    "cisne",
    "colmena",
    "cometa",
    "corcho",
    "cordel",
    "cristal",
    "cuarzo",
    "cueva",
    "cumbre",
    "desierto",
    "diamante",
    "eclipse",
    "encina",
    "esmeralda",
    "espuma",
    "estrella",
    "fogata",
    "frambuesa",
    "gaviota",
    "girasol",
    "granito",
    "hoguera",
    "volcan",
];

/// The C#'s Latin fallback, `FallbackLatinWords`.
const FALLBACK_LATIN: &[&str] = &[
    "aqua", "arbor", "ardor", "astrum", "aurora", "avis", "bellum", "caelum", "campus", "candela",
    "carmen", "castrum", "causa", "civis", "clamor", "corona", "corpus", "cursus", "decus",
    "dominus", "donum", "ferrum", "fides", "flamma", "flumen", "fortuna", "forum", "fulmen",
    "gloria", "gratia", "herba", "hortus", "ignis", "imperium", "insula", "lumen", "luna",
    "magister", "mare", "memoria", "navis", "nebula", "nomen", "oculus", "populus", "portus",
    "ratio", "regnum", "sagitta", "scutum",
];

/// A passphrase language, as the C# `PassphraseLanguage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassphraseLanguage {
    /// The interface language that starts on it.
    pub locale: &'static str,
    /// Its list, as built in.
    text: &'static str,
    /// Its fallback.
    fallback: &'static [&'static str],
}

/// The languages, in the box's order, as the C# `PassphraseLanguages`.
pub const PASSPHRASE_LANGUAGES: [PassphraseLanguage; 4] = [
    PassphraseLanguage {
        locale: "en",
        text: include_str!("../../assets/wordlists/wordlist_en.txt"),
        fallback: FALLBACK_ENGLISH,
    },
    PassphraseLanguage {
        locale: "fr",
        text: include_str!("../../assets/wordlists/wordlist_fr.txt"),
        fallback: FALLBACK_FRENCH,
    },
    PassphraseLanguage {
        locale: "es",
        text: include_str!("../../assets/wordlists/wordlist_es.txt"),
        fallback: FALLBACK_SPANISH,
    },
    PassphraseLanguage {
        locale: "la",
        text: include_str!("../../assets/wordlists/wordlist_la.txt"),
        fallback: FALLBACK_LATIN,
    },
];

/// The language index an interface in `locale` starts on, English when it has no list, as
/// the C# `PassphraseLanguageIndexFor`.
#[must_use]
pub fn language_index_for(locale: &str) -> usize {
    PASSPHRASE_LANGUAGES
        .iter()
        .position(|language| language.locale.eq_ignore_ascii_case(locale))
        .unwrap_or(0)
}

/// The words of `text` the C# loader keeps, or `fallback` when too few remain.
fn load(text: &str, fallback: &[&str]) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with(COMMENT_MARK) {
            continue;
        }
        let word = line.trim().to_lowercase();
        let length = word.encode_utf16().count();
        if (MIN_WORD_LENGTH..=MAX_WORD_LENGTH).contains(&length) && !words.contains(&word) {
            words.push(word);
        }
    }
    if words.len() >= MIN_WORDS {
        words
    } else {
        fallback.iter().map(|word| (*word).to_owned()).collect()
    }
}

/// The lists, read once.
static WORD_LISTS: LazyLock<Vec<Vec<String>>> = LazyLock::new(|| {
    PASSPHRASE_LANGUAGES
        .iter()
        .map(|language| load(language.text, language.fallback))
        .collect()
});

/// The words of the language at `index`, held to the table, as the C# `SelectedWordList`.
#[must_use]
pub fn word_list(index: usize) -> &'static [String] {
    let lists = &*WORD_LISTS;
    &lists[index.min(lists.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_list_is_the_csharp_one_typeable_and_without_repeats() {
        // The sizes the C# ships: 3525, 2774, 724 and 3649 words.
        let sizes: Vec<usize> = (0..4).map(|index| word_list(index).len()).collect();
        assert!(
            sizes[0] > 3000 && sizes[1] > 2500 && sizes[2] > 700 && sizes[3] > 3500,
            "{sizes:?}"
        );
        for index in 0..4 {
            let words = word_list(index);
            assert!(
                words
                    .iter()
                    .all(|word| word.chars().all(|c| c.is_ascii_lowercase()))
            );
            let mut sorted = words.to_vec();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), words.len());
        }
        assert_eq!(
            word_list(99),
            word_list(3),
            "an index past the table is the last"
        );
    }

    #[test]
    fn the_interface_language_picks_its_list() {
        // As the C# PassphraseLanguageIndexFor_MapsTheInterfaceLocaleToItsWordList.
        for (locale, expected) in [
            ("en", 0),
            ("fr", 1),
            ("es", 2),
            ("ES", 2),
            ("la", 3),
            ("de", 0),
        ] {
            assert_eq!(language_index_for(locale), expected);
        }
    }

    #[test]
    fn a_list_too_short_falls_back() {
        assert_eq!(load("# header\nab\nabc\n", FALLBACK_LATIN).len(), 50);
        assert_eq!(FALLBACK_ENGLISH.len(), 50);
        assert_eq!(FALLBACK_FRENCH.len(), 50);
        assert_eq!(FALLBACK_SPANISH.len(), 50);
    }
}
