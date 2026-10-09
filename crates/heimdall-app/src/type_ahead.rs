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

//! Type-ahead over a list: the first letters of a name typed select it, as WPF's
//! `TextSearch` does on the C# file lists (`EmbeddedSftpView.xaml:568-569` and
//! `LocalFileBrowserView.xaml:110-111`: `TextSearch.TextPath="Name"`,
//! `IsTextSearchEnabled="True"`).
//!
//! As WPF's: the characters typed add up to a prefix, matched whatever its case against the
//! start of each name, from the entry matched last round to it again past the end; with
//! no search going on, from the first entry. A character no name continues the prefix with
//! leaves the search as it was. The same character again, when no name starts with the
//! prefix it would make, goes on to the next name starting with the prefix as it is. A pause
//! longer than [`TYPE_AHEAD_RESET`] ends the search. A space never starts one: the list
//! keeps it, as WPF's `ListBox` takes it to select the entry at the cursor.

use std::time::{Duration, Instant};

/// Pause after which the characters typed start a new search: WPF's `TextSearch` timeout,
/// twice the system's double-click time, which is half a second by default.
pub const TYPE_AHEAD_RESET: Duration = Duration::from_secs(1);

/// What a space types, which never starts a search.
const SPACE: &str = " ";

/// The search going on over one list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeAhead {
    /// The characters matched so far, in lower case.
    prefix: String,
    /// The last character that lengthened the prefix, in lower case.
    last: Option<String>,
    /// The entry matched last: set while a search goes on.
    matched: Option<usize>,
    /// When the last character was typed while a search went on.
    at: Option<Instant>,
}

impl TypeAhead {
    /// `typed` at `now` over a list whose names are `names`, in its order: the entry to
    /// select, when the selection moves.
    pub fn typed<S: AsRef<str>>(
        &mut self,
        typed: &str,
        now: Instant,
        names: &[S],
    ) -> Option<usize> {
        if self
            .at
            .is_some_and(|at| now.saturating_duration_since(at) > TYPE_AHEAD_RESET)
        {
            *self = Self::default();
        }
        if typed.is_empty() || (self.prefix.is_empty() && typed == SPACE) {
            return None;
        }
        let typed = typed.to_lowercase();
        let count = names.len();
        let start = self.matched.filter(|index| *index < count).unwrap_or(0);
        let repeated = self.last.as_deref() == Some(typed.as_str());
        let wanted = format!("{}{typed}", self.prefix);
        let mut found = None;
        let mut fallback = None;
        for step in 0..count {
            let index = (start + step) % count;
            let name = names[index].as_ref().to_lowercase();
            if name.starts_with(&wanted) {
                found = Some(index);
                break;
            }
            // The entry matched itself is no next one.
            if repeated
                && step > 0
                && fallback.is_none()
                && !self.prefix.is_empty()
                && name.starts_with(&self.prefix)
            {
                fallback = Some(index);
            }
        }
        let mut moved = None;
        if let Some(index) = found.or(fallback) {
            if self.matched.is_none() || index != start {
                moved = Some(index);
            }
            self.matched = Some(index);
            if found.is_some() {
                self.prefix = wanted;
                self.last = Some(typed);
            }
        }
        if self.matched.is_some() {
            self.at = Some(now);
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pause shorter than the reset.
    const SHORT: Duration = Duration::from_millis(200);

    const NAMES: &[&str] = &[
        "bin",
        "Backup.tar",
        "boot",
        "etc",
        "home",
        "srv",
        "SSH",
        "sys",
    ];

    /// `typed` one after the other, [`SHORT`] apart from `from`: where the selection moved
    /// each time.
    fn run(search: &mut TypeAhead, from: Instant, typed: &[&str]) -> Vec<Option<usize>> {
        typed
            .iter()
            .enumerate()
            .map(|(step, text)| {
                let at = from + SHORT * u32::try_from(step).expect("few steps");
                search.typed(text, at, NAMES)
            })
            .collect()
    }

    #[test]
    fn the_letters_typed_add_up_to_a_prefix_whatever_their_case() {
        let mut search = TypeAhead::default();
        let now = Instant::now();
        assert_eq!(run(&mut search, now, &["S", "y"]), [Some(5), Some(7)]);
    }

    #[test]
    fn a_new_search_starts_from_the_first_entry() {
        let mut search = TypeAhead::default();
        let now = Instant::now();
        assert_eq!(run(&mut search, now, &["h"]), [Some(4)]);
        // Past the pause, from the first entry, not from the one matched.
        assert_eq!(
            search.typed("b", now + TYPE_AHEAD_RESET * 2, NAMES),
            Some(0)
        );
    }

    #[test]
    fn a_name_starting_with_the_letter_doubled_is_matched_before_the_next_one() {
        let mut search = TypeAhead::default();
        let now = Instant::now();
        // "ss" makes the prefix "SSH" starts with; "sss" matches nothing, and no other name
        // starts with "ss": the selection stays, as WPF's.
        assert_eq!(
            run(&mut search, now, &["s", "s", "s"]),
            [Some(5), Some(6), None]
        );
    }

    #[test]
    fn the_same_letter_again_goes_on_to_the_next_name_and_wraps_round_the_list() {
        let mut search = TypeAhead::default();
        let now = Instant::now();
        assert_eq!(
            run(&mut search, now, &["b", "b", "b", "b"]),
            [Some(0), Some(1), Some(2), Some(0)]
        );
    }

    #[test]
    fn a_letter_matching_nothing_keeps_the_search_as_it_was() {
        let mut search = TypeAhead::default();
        let now = Instant::now();
        assert_eq!(
            run(&mut search, now, &["h", "x", "o"]),
            [Some(4), None, None]
        );
        // "ho" matched "home" again: the prefix kept going.
        assert_eq!(search.typed("m", now + SHORT * 3, NAMES), None);
        assert_eq!(search.typed("z", now + SHORT * 4, NAMES), None);
        // Nothing matched first: no search goes on, the next letter starts one.
        let mut search = TypeAhead::default();
        assert_eq!(run(&mut search, now, &["x", "e"]), [None, Some(3)]);
    }

    #[test]
    fn a_pause_longer_than_the_reset_starts_a_new_search() {
        let mut search = TypeAhead::default();
        let now = Instant::now();
        assert_eq!(search.typed("s", now, NAMES), Some(5));
        // Within the pause: "sy".
        assert_eq!(search.typed("y", now + TYPE_AHEAD_RESET, NAMES), Some(7));
        // The pause measured from the last letter: "e" alone, from the top.
        let later = now + TYPE_AHEAD_RESET + TYPE_AHEAD_RESET + SHORT;
        assert_eq!(search.typed("e", later, NAMES), Some(3));
    }

    #[test]
    fn a_space_goes_on_with_a_search_but_never_starts_one() {
        let names = ["My Documents", "My Music", "Myrtle"];
        let mut search = TypeAhead::default();
        let now = Instant::now();
        assert_eq!(search.typed(" ", now, &names), None);
        assert_eq!(search.typed("m", now, &names), Some(0));
        assert_eq!(search.typed("y", now, &names), None, "still the first");
        assert_eq!(search.typed(" ", now, &names), None, "still the first");
        assert_eq!(search.typed("m", now, &names), Some(1));
    }

    #[test]
    fn an_empty_list_matches_nothing() {
        let mut search = TypeAhead::default();
        let names: [&str; 0] = [];
        assert_eq!(search.typed("a", Instant::now(), &names), None);
    }
}
