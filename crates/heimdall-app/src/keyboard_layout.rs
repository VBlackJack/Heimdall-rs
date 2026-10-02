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

//! The keyboard layout this computer types with, as an RDP server is told it, as mstsc
//! does. The keys go to the server as the positions pressed; the server reads them through
//! the layout it is told, so a French AZERTY keyboard announced as nothing types QWERTY
//! letters, a password on the remote sign-in screen included.
//!
//! On Windows, the user's default input layout: the first one the registry lists, through
//! its substitute when there is one (a variant of the language's layout, Swiss French for
//! one). Elsewhere, none: the server's own.

/// Said when no layout is known: the server uses its own.
pub const SERVER_DEFAULT: u32 = 0;

/// The user's input layouts, in the order the language bar shows them.
#[cfg(windows)]
const PRELOAD_KEY: &str = r"Keyboard Layout\Preload";

/// The variant used for a layout of the list, by that layout's identifier.
#[cfg(windows)]
const SUBSTITUTES_KEY: &str = r"Keyboard Layout\Substitutes";

/// The first, default layout of the list.
#[cfg(windows)]
const DEFAULT_ENTRY: &str = "1";

/// Hexadecimal, as the registry writes layout identifiers.
const HEX: u32 = 16;

/// The layout the server is told: this computer's, or [`SERVER_DEFAULT`].
#[must_use]
pub fn local() -> u32 {
    #[cfg(windows)]
    {
        use windows_registry::CURRENT_USER;
        let read = |key: &str, name: &str| {
            CURRENT_USER
                .open(key)
                .and_then(|key| key.get_string(name))
                .ok()
        };
        let Some(listed) = read(PRELOAD_KEY, DEFAULT_ENTRY) else {
            return SERVER_DEFAULT;
        };
        let substitute = read(SUBSTITUTES_KEY, &listed);
        chosen(&listed, substitute.as_deref())
    }
    #[cfg(not(windows))]
    {
        SERVER_DEFAULT
    }
}

/// The layout identifier for `listed`, through `substitute` when there is a readable one.
#[must_use]
pub fn chosen(listed: &str, substitute: Option<&str>) -> u32 {
    substitute
        .and_then(identifier)
        .or_else(|| identifier(listed))
        .unwrap_or(SERVER_DEFAULT)
}

/// A layout identifier as the registry writes it, eight hexadecimal digits.
fn identifier(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() || text.len() > 8 {
        return None;
    }
    u32::from_str_radix(text, HEX).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listed_layout_is_told_through_its_substitute_when_it_has_one() {
        // French (France), AZERTY.
        assert_eq!(chosen("0000040c", None), 0x0000_040c);
        // Swiss French, a variant written as a substitute of French.
        assert_eq!(chosen("0000100c", Some("0000100c")), 0x0000_100c);
        assert_eq!(chosen("0000040c", Some("0001040c")), 0x0001_040c);
        // A substitute that does not read leaves the listed layout.
        assert_eq!(chosen("0000040c", Some("azerty")), 0x0000_040c);
    }

    #[test]
    fn what_does_not_read_leaves_the_server_its_own_layout() {
        for unreadable in ["", "  ", "0x040c", "0000040c0", "zz"] {
            assert_eq!(chosen(unreadable, None), SERVER_DEFAULT, "{unreadable:?}");
        }
        assert_eq!(chosen(" 0000080c ", None), 0x0000_080c, "spaces around");
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_user_with_a_layout_listed_tells_it() {
        use windows_registry::CURRENT_USER;
        let listed = CURRENT_USER
            .open(PRELOAD_KEY)
            .and_then(|key| key.get_string(DEFAULT_ENTRY));
        if let Ok(listed) = listed {
            assert_ne!(local(), SERVER_DEFAULT, "listed: {listed}");
        }
    }
}
