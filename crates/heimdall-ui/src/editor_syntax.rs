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

//! The language a file edited is coloured as, and named in the editor's status bar, as the
//! C# editor's syntax name. The syntaxes are the highlighter's own, so the name said is
//! the colouring seen.

use std::sync::LazyLock;

use two_face::re_exports::syntect::parsing::SyntaxSet;

/// The syntaxes known, those the editor's highlighter loads.
static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_no_newlines);

/// What the highlighter is given for a text with no language.
const PLAIN_TOKEN: &str = "txt";

/// How a file is coloured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Syntax {
    /// What the highlighter is given to find it.
    pub token: String,
    /// Its name; `None` for plain text.
    pub name: Option<String>,
}

/// The syntax of the file named `name`: by its whole name first, which knows a
/// `Dockerfile`, a `.bashrc` or an `sshd_config`, then by its extension; else plain text.
#[must_use]
pub fn of(name: &str) -> Syntax {
    let plain = SYNTAXES.find_syntax_plain_text().name.as_str();
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    [Some(name), extension]
        .into_iter()
        .flatten()
        .filter(|token| !token.is_empty())
        .find_map(|token| {
            let syntax = SYNTAXES.find_syntax_by_token(token)?;
            (syntax.name != plain).then(|| Syntax {
                token: token.to_owned(),
                name: Some(syntax.name.clone()),
            })
        })
        .unwrap_or_else(|| Syntax {
            token: PLAIN_TOKEN.to_owned(),
            name: None,
        })
}

#[cfg(test)]
mod tests {
    use super::of;

    #[test]
    fn a_file_is_named_by_its_extension_or_its_whole_name() {
        let name = |file: &str| of(file).name;
        assert_eq!(name("main.rs").as_deref(), Some("Rust"));
        assert_eq!(name("config.toml").as_deref(), Some("TOML"));
        assert!(name(".bashrc").is_some_and(|name| name.contains("bash")));
        assert_eq!(name("Dockerfile").as_deref(), Some("Dockerfile"));
        assert_eq!(name("settings.json").as_deref(), Some("JSON"));
    }

    #[test]
    fn a_file_with_no_language_is_plain_text() {
        for file in ["notes.txt", "README", "data.unknownext", ""] {
            let syntax = of(file);
            assert_eq!(syntax.name, None, "{file}");
            assert_eq!(syntax.token, "txt", "{file}");
        }
    }
}
