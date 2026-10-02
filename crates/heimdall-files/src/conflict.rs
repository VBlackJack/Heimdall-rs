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

//! What to do with destinations already taken, decided for a whole transfer before it
//! starts, as the C# `FileConflictPlanner`: each planned entry is checked against what
//! exists and against the entries planned before it; the user picks Skip, Replace or
//! Auto-rename for each conflict; skipping a folder skips everything planned inside it.
//!
//! Pure: the caller says what exists and how names compare (exactly on a server, without
//! case on Windows).

use std::collections::{HashMap, HashSet};

/// A destination, as its names from the transfer's destination folder down.
pub type Target = Vec<Vec<u8>>;

/// Added to a name auto-renamed: "report (copy).pdf", then "report (copy 2).pdf".
const COPY_MARK: &[u8] = b" (copy";

/// The first number written in a copy's name.
const FIRST_NUMBERED_COPY: u64 = 2;

/// What a destination is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A file, or anything that is not a folder.
    File,
    /// A folder.
    Folder,
}

/// What a planned entry does with its destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// A transfer: a folder going into a folder already there adds to it.
    Transfer,
    /// A rename: the destination is the entry itself, never added to.
    Rename,
}

/// What the user may choose for one conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    /// Leave the entry out.
    pub skip: bool,
    /// Replace what is there.
    pub replace: bool,
    /// Use a free name next to it.
    pub rename: bool,
}

impl Allowed {
    const ALL: Self = Self {
        skip: true,
        replace: true,
        rename: true,
    };
    const NONE: Self = Self {
        skip: false,
        replace: false,
        rename: false,
    };
    const SKIP_OR_RENAME: Self = Self {
        skip: true,
        replace: false,
        rename: true,
    };
    const SKIP: Self = Self {
        skip: true,
        replace: false,
        rename: false,
    };

    /// Whether `choice` is one of them.
    #[must_use]
    pub const fn allows(self, choice: Choice) -> bool {
        match choice {
            Choice::Skip => self.skip,
            Choice::Replace => self.replace,
            Choice::AutoRename => self.rename,
        }
    }

    /// The choice a conflict starts with: Auto-rename when allowed, as the C#, since it
    /// loses nothing; otherwise the first allowed.
    #[must_use]
    pub const fn default_choice(self) -> Option<Choice> {
        if self.rename {
            Some(Choice::AutoRename)
        } else if self.skip {
            Some(Choice::Skip)
        } else if self.replace {
            Some(Choice::Replace)
        } else {
            None
        }
    }

    const fn and(self, other: Self) -> Self {
        Self {
            skip: self.skip && other.skip,
            replace: self.replace && other.replace,
            rename: self.rename && other.rename,
        }
    }

    const fn is_none(self) -> bool {
        !self.skip && !self.replace && !self.rename
    }
}

/// The user's answer to one conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Leave the entry out.
    Skip,
    /// Replace what is there.
    Replace,
    /// Use a free name next to it.
    AutoRename,
}

/// An entry the transfer plans to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    /// Where it goes.
    pub target: Target,
    /// What it is.
    pub kind: Kind,
}

/// A planned entry, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// Where it goes.
    pub target: Target,
    /// What it is.
    pub kind: Kind,
    /// What is in the way, when something is: what exists there, or an entry planned before
    /// it.
    pub in_the_way: Option<Kind>,
    /// What the user may choose; [`None`] when nothing is in the way.
    pub allowed: Option<Allowed>,
}

impl Checked {
    /// Whether the user must choose.
    #[must_use]
    pub const fn is_conflict(&self) -> bool {
        self.allowed.is_some()
    }
}

/// What the transfer does with one planned entry once every conflict is answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Write it at its target; `replace` only when the user chose so.
    Write {
        /// Replace what is there.
        replace: bool,
    },
    /// Write it at this free name instead.
    WriteAs(Target),
    /// Leave it out.
    Skip,
}

/// Why answers cannot be applied: a fault of the caller, never of the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    /// A conflict has no answer.
    #[error("conflict {0} has no answer")]
    Unanswered(usize),
    /// An answer is for an entry that is not a conflict, or out of range.
    #[error("entry {0} is not a conflict")]
    NotAConflict(usize),
    /// An answer is not one the conflict allows.
    #[error("the answer to conflict {0} is not allowed")]
    NotAllowed(usize),
}

/// Checks each planned entry against what exists and the entries planned before it.
/// `existing` says what is at a target; `fold` gives the form names compare by.
pub fn check(
    planned: &[Planned],
    mut existing: impl FnMut(&Target) -> Option<Kind>,
    fold: impl Fn(&[u8]) -> Vec<u8>,
    policy: Policy,
) -> Vec<Checked> {
    let mut claimed: HashMap<Target, Kind> = HashMap::new();
    planned
        .iter()
        .map(|entry| {
            let key = folded(&entry.target, &fold);
            let mut allowed = Allowed::ALL;
            let mut in_the_way = None;
            let found = existing(&entry.target);
            let earlier = claimed.get(&key).copied();
            for other in [found, earlier].into_iter().flatten() {
                let actions = allowed_for(policy, entry.kind, other);
                if actions.is_none() {
                    continue;
                }
                allowed = allowed.and(actions);
                in_the_way.get_or_insert(other);
            }
            claimed.entry(key).or_insert(entry.kind);
            let conflict = in_the_way.is_some();
            Checked {
                target: entry.target.clone(),
                kind: entry.kind,
                in_the_way: in_the_way.or(found).or(earlier),
                allowed: conflict.then_some(allowed),
            }
        })
        .collect()
}

/// Applies the user's answers, `(entry index, choice)` for every conflict; returns what to
/// do with each planned entry, in order. `exists` says whether a target is taken, for the
/// free names.
///
/// # Errors
///
/// [`PlanError`] for answers missing, extra, or not allowed.
pub fn resolve(
    checked: &[Checked],
    answers: &[(usize, Choice)],
    mut exists: impl FnMut(&Target) -> bool,
    fold: impl Fn(&[u8]) -> Vec<u8>,
) -> Result<Vec<Outcome>, PlanError> {
    let mut chosen = HashMap::new();
    let mut skipped_folders = Vec::new();
    for &(index, choice) in answers {
        let entry = checked.get(index).ok_or(PlanError::NotAConflict(index))?;
        let allowed = entry.allowed.ok_or(PlanError::NotAConflict(index))?;
        if !allowed.allows(choice) {
            return Err(PlanError::NotAllowed(index));
        }
        if entry.kind == Kind::Folder && choice == Choice::Skip {
            skipped_folders.push(folded(&entry.target, &fold));
        }
        chosen.insert(index, choice);
    }
    let mut reserved: HashSet<Target> = checked
        .iter()
        .map(|entry| folded(&entry.target, &fold))
        .collect();
    let mut outcomes = Vec::with_capacity(checked.len());
    for (index, entry) in checked.iter().enumerate() {
        let key = folded(&entry.target, &fold);
        if skipped_folders
            .iter()
            .any(|folder| key.len() > folder.len() && key.starts_with(folder))
        {
            outcomes.push(Outcome::Skip);
            continue;
        }
        if !entry.is_conflict() {
            outcomes.push(Outcome::Write { replace: false });
            continue;
        }
        let choice = chosen
            .get(&index)
            .copied()
            .ok_or(PlanError::Unanswered(index))?;
        outcomes.push(match choice {
            Choice::Skip => Outcome::Skip,
            Choice::Replace => Outcome::Write { replace: true },
            Choice::AutoRename => {
                let free = free_target(&entry.target, &reserved, &mut exists, &fold);
                reserved.insert(folded(&free, &fold));
                Outcome::WriteAs(free)
            }
        });
    }
    Ok(outcomes)
}

/// The names tried for a copy of `name`, in order: "report (copy).pdf", then
/// "report (copy 2).pdf", without end.
pub fn copy_names(name: &[u8]) -> impl Iterator<Item = Vec<u8>> + '_ {
    let (stem, extension) = split_extension(name);
    std::iter::once(None)
        .chain((FIRST_NUMBERED_COPY..).map(Some))
        .map(move |number| {
            let mut copy = stem.to_vec();
            copy.extend_from_slice(COPY_MARK);
            if let Some(number) = number {
                copy.push(b' ');
                copy.extend_from_slice(number.to_string().as_bytes());
            }
            copy.push(b')');
            copy.extend_from_slice(extension);
            copy
        })
}

/// What `kind` may do over `other` already there.
const fn allowed_for(policy: Policy, kind: Kind, other: Kind) -> Allowed {
    match (policy, kind, other) {
        (Policy::Transfer | Policy::Rename, Kind::File, Kind::File) => Allowed::ALL,
        // Added to, not replaced: no conflict.
        (Policy::Transfer, Kind::Folder, Kind::Folder) => Allowed::NONE,
        (Policy::Transfer, Kind::File, Kind::Folder) | (Policy::Rename, _, _) => {
            Allowed::SKIP_OR_RENAME
        }
        (Policy::Transfer, Kind::Folder, Kind::File) => Allowed::SKIP,
    }
}

fn folded(target: &Target, fold: &impl Fn(&[u8]) -> Vec<u8>) -> Target {
    target.iter().map(|name| fold(name)).collect()
}

/// The first copy name free in the destination and among the names already planned.
fn free_target(
    target: &Target,
    reserved: &HashSet<Target>,
    exists: &mut impl FnMut(&Target) -> bool,
    fold: &impl Fn(&[u8]) -> Vec<u8>,
) -> Target {
    let (leaf, parent) = target
        .split_last()
        .map_or((&[][..], &[][..]), |(leaf, parent)| {
            (leaf.as_slice(), parent)
        });
    copy_names(leaf)
        .map(|name| {
            let mut candidate = parent.to_vec();
            candidate.push(name);
            candidate
        })
        .find(|candidate| !reserved.contains(&folded(candidate, fold)) && !exists(candidate))
        .unwrap_or_else(|| unreachable!("the copy names never end"))
}

/// A name's stem and extension, the dot kept with the extension; a leading dot, as in
/// ".bashrc", or a trailing one is part of the stem.
fn split_extension(name: &[u8]) -> (&[u8], &[u8]) {
    match name.iter().rposition(|&byte| byte == b'.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => name.split_at(dot),
        _ => (name, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(path: &str) -> Target {
        path.split('/')
            .map(|name| name.as_bytes().to_vec())
            .collect()
    }

    fn file(path: &str) -> Planned {
        Planned {
            target: target(path),
            kind: Kind::File,
        }
    }

    fn folder(path: &str) -> Planned {
        Planned {
            target: target(path),
            kind: Kind::Folder,
        }
    }

    fn exact(name: &[u8]) -> Vec<u8> {
        name.to_vec()
    }

    fn caseless(name: &[u8]) -> Vec<u8> {
        name.to_ascii_lowercase()
    }

    fn on_disk<'a>(entries: &'a [(&'a str, Kind)]) -> impl FnMut(&Target) -> Option<Kind> + 'a {
        move |wanted| {
            entries
                .iter()
                .find(|(path, _)| target(path) == *wanted)
                .map(|&(_, kind)| kind)
        }
    }

    #[test]
    fn copy_names_keep_the_extension_and_count_from_two() {
        let names: Vec<_> = copy_names(b"report.pdf").take(3).collect();
        assert_eq!(
            names,
            [
                b"report (copy).pdf".to_vec(),
                b"report (copy 2).pdf".to_vec(),
                b"report (copy 3).pdf".to_vec()
            ]
        );
        let dotted = |name: &[u8]| copy_names(name).next().expect("first");
        assert_eq!(dotted(b".bashrc"), b".bashrc (copy)");
        assert_eq!(dotted(b"notes."), b"notes. (copy)");
        assert_eq!(dotted(b"archive.tar.gz"), b"archive.tar (copy).gz");
        assert_eq!(dotted(b"README"), b"README (copy)");
    }

    #[test]
    fn the_kind_matrix_is_the_csharp_one() {
        use Kind::{File, Folder};
        use Policy::{Rename, Transfer};

        let one = |kind, other, policy| {
            check(
                &[Planned {
                    target: target("x"),
                    kind,
                }],
                |_| Some(other),
                exact,
                policy,
            )
            .remove(0)
            .allowed
        };
        assert_eq!(one(File, File, Transfer), Some(Allowed::ALL));
        assert_eq!(one(Folder, Folder, Transfer), None, "added to");
        assert_eq!(one(File, Folder, Transfer), Some(Allowed::SKIP_OR_RENAME));
        assert_eq!(one(Folder, File, Transfer), Some(Allowed::SKIP));
        assert_eq!(one(File, File, Rename), Some(Allowed::ALL));
        assert_eq!(one(Folder, Folder, Rename), Some(Allowed::SKIP_OR_RENAME));
        assert_eq!(one(File, Folder, Rename), Some(Allowed::SKIP_OR_RENAME));
        assert_eq!(one(Folder, File, Rename), Some(Allowed::SKIP_OR_RENAME));
    }

    #[test]
    fn a_target_planned_twice_is_a_conflict_the_second_time() {
        let checked = check(
            &[file("a.txt"), file("A.TXT"), file("b.txt")],
            |_| None,
            caseless,
            Policy::Transfer,
        );
        let conflicts: Vec<_> = checked.iter().map(Checked::is_conflict).collect();
        assert_eq!(conflicts, [false, true, false]);
        assert_eq!(checked[1].in_the_way, Some(Kind::File));
        let exactly = check(
            &[file("a.txt"), file("A.TXT")],
            |_| None,
            exact,
            Policy::Transfer,
        );
        assert!(!exactly[1].is_conflict(), "a server tells case apart");
    }

    #[test]
    fn existing_and_planned_conflicts_narrow_the_choices_together() {
        // A file where a folder exists, and planned after a file of the same name.
        let entries = [("x", Kind::Folder)];
        let checked = check(
            &[file("x"), file("x")],
            on_disk(&entries),
            exact,
            Policy::Transfer,
        );
        assert_eq!(checked[1].allowed, Some(Allowed::SKIP_OR_RENAME));
        assert_eq!(
            checked[1].in_the_way,
            Some(Kind::Folder),
            "what exists first"
        );
    }

    #[test]
    fn answers_become_outcomes_and_a_copy_name_is_free_everywhere() {
        let entries = [
            ("a.txt", Kind::File),
            ("b.txt", Kind::File),
            ("c.txt", Kind::File),
            ("c (copy).txt", Kind::File),
        ];
        let planned = [
            file("a.txt"),
            file("b.txt"),
            file("c.txt"),
            // Planned under the name the copy of c would take first after the one taken.
            file("c (copy 2).txt"),
            file("new.txt"),
        ];
        let checked = check(&planned, on_disk(&entries), exact, Policy::Transfer);
        let mut taken = on_disk(&entries);
        let outcomes = resolve(
            &checked,
            &[
                (0, Choice::Skip),
                (1, Choice::Replace),
                (2, Choice::AutoRename),
            ],
            |candidate| taken(candidate).is_some(),
            exact,
        )
        .expect("resolved");
        assert_eq!(
            outcomes,
            [
                Outcome::Skip,
                Outcome::Write { replace: true },
                Outcome::WriteAs(target("c (copy 3).txt")),
                Outcome::Write { replace: false },
                Outcome::Write { replace: false },
            ]
        );
    }

    #[test]
    fn two_copies_of_one_name_never_take_the_same_free_name() {
        let entries = [("a.txt", Kind::File)];
        let checked = check(
            &[file("a.txt"), file("a.txt")],
            on_disk(&entries),
            exact,
            Policy::Transfer,
        );
        let outcomes = resolve(
            &checked,
            &[(0, Choice::AutoRename), (1, Choice::AutoRename)],
            |_| false,
            exact,
        )
        .expect("resolved");
        assert_eq!(
            outcomes,
            [
                Outcome::WriteAs(target("a (copy).txt")),
                Outcome::WriteAs(target("a (copy 2).txt")),
            ]
        );
    }

    #[test]
    fn skipping_a_folder_skips_everything_planned_inside_it_only() {
        let entries = [("docs", Kind::File)];
        let planned = [
            folder("docs"),
            file("docs/a.txt"),
            folder("docs/deep"),
            file("docs/deep/b.txt"),
            file("docsx.txt"),
            file("DOCS/c.txt"),
        ];
        let checked = check(&planned, on_disk(&entries), caseless, Policy::Transfer);
        assert_eq!(
            checked[0].allowed,
            Some(Allowed::SKIP),
            "a folder over a file"
        );
        let outcomes =
            resolve(&checked, &[(0, Choice::Skip)], |_| false, caseless).expect("resolved");
        assert_eq!(
            outcomes,
            [
                Outcome::Skip,
                Outcome::Skip,
                Outcome::Skip,
                Outcome::Skip,
                Outcome::Write { replace: false },
                Outcome::Skip,
            ]
        );
    }

    #[test]
    fn answers_that_do_not_fit_are_refused() {
        let entries = [("a", Kind::Folder)];
        let checked = check(
            &[file("a"), file("b")],
            on_disk(&entries),
            exact,
            Policy::Transfer,
        );
        let answer = |answers: &[(usize, Choice)]| resolve(&checked, answers, |_| false, exact);
        assert_eq!(answer(&[]), Err(PlanError::Unanswered(0)));
        assert_eq!(
            answer(&[(0, Choice::Replace)]),
            Err(PlanError::NotAllowed(0)),
            "a file never replaces a folder"
        );
        assert_eq!(
            answer(&[(0, Choice::Skip), (1, Choice::Skip)]),
            Err(PlanError::NotAConflict(1))
        );
        assert_eq!(
            answer(&[(0, Choice::Skip), (9, Choice::Skip)]),
            Err(PlanError::NotAConflict(9))
        );
    }

    #[test]
    fn a_conflict_starts_on_auto_rename_when_it_may() {
        assert_eq!(Allowed::ALL.default_choice(), Some(Choice::AutoRename));
        assert_eq!(Allowed::SKIP.default_choice(), Some(Choice::Skip));
        assert_eq!(Allowed::NONE.default_choice(), None);
        let replace_only = Allowed {
            skip: false,
            replace: true,
            rename: false,
        };
        assert_eq!(replace_only.default_choice(), Some(Choice::Replace));
    }
}
