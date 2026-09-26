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

use std::path::PathBuf;

use xtask::legacy_locales::{Translation, lookup};

fn legacy_repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("legacy")
}

fn translation(language: &'static str, value: Option<&str>) -> Translation {
    Translation {
        language,
        value: value.map(str::to_owned),
    }
}

#[test]
fn a_key_is_returned_in_every_supported_language() {
    let found = lookup(&legacy_repo(), "TreeUxConnect").expect("fixtures are valid");
    assert_eq!(
        found,
        vec![
            translation("en", Some("Connect")),
            translation("fr", Some("Se connecter")),
            translation("es", Some("Conectar")),
        ]
    );
}

#[test]
fn a_language_that_lacks_the_key_says_so() {
    let found = lookup(&legacy_repo(), "OnlyEnglish").expect("fixtures are valid");
    assert_eq!(
        found,
        vec![
            translation("en", Some("Only here")),
            translation("fr", None),
            translation("es", None),
        ]
    );
}

#[test]
fn a_missing_repository_is_an_error() {
    assert!(lookup(&legacy_repo().join("absent"), "TreeUxConnect").is_err());
}
