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

//! A file's permission bits as the chmod calculator reads and writes them, as the C#
//! `PosixMode` (`Heimdall.Core/Permissions/PosixMode.cs`) and `SymbolicChmodParser`
//! (`SymbolicChmodParser.cs`): three octal digits, the owner's, the group's and the others',
//! each a sum of read (4), write (2) and execute (1).

/// The bit of read in a digit.
const READ_BIT: u8 = 4;

/// The bit of write in a digit.
const WRITE_BIT: u8 = 2;

/// The bit of execute in a digit.
const EXECUTE_BIT: u8 = 1;

/// The largest digit.
const MAX_DIGIT: u8 = 7;

/// Digits of an octal mode.
const OCTAL_LENGTH: usize = 3;

/// The radix of a mode's digits.
const OCTAL_RADIX: u32 = 8;

/// Who a permission is for, as the C# `PosixRole`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosixRole {
    /// The file's owner, `u`.
    Owner,
    /// Its group, `g`.
    Group,
    /// Everyone else, `o`.
    Others,
}

impl PosixRole {
    /// The three, in the order the mode writes them.
    pub const ALL: [Self; 3] = [Self::Owner, Self::Group, Self::Others];

    /// The letter of a symbolic clause that names it.
    const fn letter(self) -> char {
        match self {
            Self::Owner => 'u',
            Self::Group => 'g',
            Self::Others => 'o',
        }
    }
}

/// A permission, as the C# `PosixPermission`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosixPermission {
    /// Read, `r`.
    Read,
    /// Write, `w`.
    Write,
    /// Execute, `x`.
    Execute,
}

impl PosixPermission {
    /// The three, in the order the mode writes them.
    pub const ALL: [Self; 3] = [Self::Read, Self::Write, Self::Execute];

    /// Its bit in a digit.
    const fn mask(self) -> u8 {
        match self {
            Self::Read => READ_BIT,
            Self::Write => WRITE_BIT,
            Self::Execute => EXECUTE_BIT,
        }
    }

    /// Its letter, in a symbolic clause and in the `rwx` form.
    const fn letter(self) -> char {
        match self {
            Self::Read => 'r',
            Self::Write => 'w',
            Self::Execute => 'x',
        }
    }
}

/// The letter `rwx` shows for a permission not given.
const NOT_GIVEN: char = '-';

/// Three permission digits, as the C# `PosixMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PosixMode {
    owner: u8,
    group: u8,
    others: u8,
}

impl PosixMode {
    /// No permission, as the C# `Empty`.
    pub const EMPTY: Self = Self::of(0, 0, 0);

    /// The presets the calculator offers, as the C# `Preset644` to `Preset777`
    /// (`PosixMode.cs:36-40`), in its order.
    pub const PRESETS: [Self; 5] = [
        Self::of(6, 4, 4),
        Self::of(7, 5, 5),
        Self::of(6, 0, 0),
        Self::of(7, 0, 0),
        Self::of(7, 7, 7),
    ];

    /// The mode a new calculator shows, as the C# `ApplyPrefill`'s `Preset755`.
    pub const DEFAULT: Self = Self::of(7, 5, 5);

    /// The mode of these digits, each kept within 0 to 7.
    #[must_use]
    pub const fn of(owner: u8, group: u8, others: u8) -> Self {
        Self {
            owner: owner & MAX_DIGIT,
            group: group & MAX_DIGIT,
            others: others & MAX_DIGIT,
        }
    }

    /// The digit of `role`.
    #[must_use]
    pub const fn digit(self, role: PosixRole) -> u8 {
        match role {
            PosixRole::Owner => self.owner,
            PosixRole::Group => self.group,
            PosixRole::Others => self.others,
        }
    }

    /// Whether `role` has `permission`.
    #[must_use]
    pub const fn has(self, role: PosixRole, permission: PosixPermission) -> bool {
        self.digit(role) & permission.mask() != 0
    }

    /// The mode three octal digits write, as the C# `TryParseOctal`
    /// (`PosixMode.cs:69-87`): exactly three, each 0 to 7; `None` for anything else.
    #[must_use]
    pub fn parse_octal(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != OCTAL_LENGTH {
            return None;
        }
        let digit = |byte: u8| {
            char::from(byte)
                .to_digit(OCTAL_RADIX)
                .and_then(|digit| u8::try_from(digit).ok())
        };
        Some(Self::of(
            digit(bytes[0])?,
            digit(bytes[1])?,
            digit(bytes[2])?,
        ))
    }

    /// Its three digits, as the C# `ToOctal`.
    #[must_use]
    pub fn to_octal(self) -> String {
        format!("{}{}{}", self.owner, self.group, self.others)
    }

    /// Its `rwxr-xr-x` form, as the C# `ToSymbolic` (`PosixMode.cs:91-98`).
    #[must_use]
    pub fn to_symbolic(self) -> String {
        PosixRole::ALL
            .into_iter()
            .flat_map(|role| {
                PosixPermission::ALL.into_iter().map(move |permission| {
                    if self.has(role, permission) {
                        permission.letter()
                    } else {
                        NOT_GIVEN
                    }
                })
            })
            .collect()
    }

    /// It with `permission` of `role` given or taken, as the C# `WithBit`
    /// (`PosixMode.cs:100-117`).
    #[must_use]
    pub const fn with_bit(self, role: PosixRole, permission: PosixPermission, on: bool) -> Self {
        let digit = self.digit(role);
        let digit = if on {
            digit | permission.mask()
        } else {
            digit & !permission.mask()
        };
        self.with_digit(role, digit)
    }

    /// It with `role`'s digit replaced.
    const fn with_digit(self, role: PosixRole, digit: u8) -> Self {
        match role {
            PosixRole::Owner => Self::of(digit, self.group, self.others),
            PosixRole::Group => Self::of(self.owner, digit, self.others),
            PosixRole::Others => Self::of(self.owner, self.group, digit),
        }
    }

    /// The mode a symbolic notation such as `u+x,g-w,o=r` makes from no permission, as the
    /// C# `SymbolicChmodParser.TryParse` (`SymbolicChmodParser.cs:23-69`): clauses split on
    /// commas, empty ones skipped, each `[ugoa]+` then `+`, `-` or `=` then `[rwx]*`, applied
    /// in turn; `None` when one is not such a clause or none is given.
    #[must_use]
    pub fn parse_symbolic(text: &str) -> Option<Self> {
        let clauses: Vec<&str> = text
            .split(',')
            .map(str::trim)
            .filter(|clause| !clause.is_empty())
            .collect();
        if clauses.is_empty() {
            return None;
        }
        clauses.into_iter().try_fold(Self::EMPTY, apply_clause)
    }
}

/// The operations of a symbolic clause.
const OPERATIONS: [char; 3] = ['+', '-', '='];

/// The letter of a clause naming every role.
const ALL_ROLES: char = 'a';

/// `mode` with `clause` applied, as the C# clause's regular expression
/// `^([ugoa]+)([\+\-=])([rwx]*)$` and `ApplyToRole` (`SymbolicChmodParser.cs:71-112`).
fn apply_clause(mode: PosixMode, clause: &str) -> Option<PosixMode> {
    let at = clause.find(OPERATIONS)?;
    let (who, rest) = clause.split_at(at);
    let mut rest = rest.chars();
    let operation = rest.next()?;
    let permissions = rest.as_str();
    let roles_ok = !who.is_empty()
        && who.chars().all(|letter| {
            letter == ALL_ROLES || PosixRole::ALL.iter().any(|r| r.letter() == letter)
        });
    let permissions_ok = permissions
        .chars()
        .all(|letter| PosixPermission::ALL.iter().any(|p| p.letter() == letter));
    if !roles_ok || !permissions_ok {
        return None;
    }
    let mut mode = mode;
    for role in PosixRole::ALL {
        if !(who.contains(role.letter()) || who.contains(ALL_ROLES)) {
            continue;
        }
        if operation == '=' {
            mode = mode.with_digit(role, 0);
        }
        for permission in PosixPermission::ALL {
            let present = permissions.contains(permission.letter());
            mode = match operation {
                '+' if present => mode.with_bit(role, permission, true),
                '-' if present => mode.with_bit(role, permission, false),
                '=' => mode.with_bit(role, permission, present),
                _ => mode,
            };
        }
    }
    Some(mode)
}
