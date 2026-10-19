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

//! A normal build started with `--data-dir`: the folder is set, every title says it, and a
//! refusal says why in the language shown. Its own process: the root, once set, is the
//! process's.

#![cfg(not(feature = "test-data-dir"))]

use std::ffi::OsString;

use heimdall_core::settings::Language;
use heimdall_ui::data_root::{ArgumentError, init, titled};

#[test]
fn a_data_dir_start_roots_the_folders_and_marks_every_title() {
    heimdall_ui::i18n::apply(Some(Language::English));
    assert_eq!(
        ArgumentError::NoValue.to_string(),
        "--data-dir needs the full path of a folder after it."
    );
    assert!(matches!(
        init([OsString::from("--data-dir"), OsString::from("relative")]),
        Err(ArgumentError::Refused(_))
    ));
    assert_eq!(titled("Heimdall".to_owned()), "Heimdall", "nothing set yet");

    let scratch = tempfile::tempdir().expect("scratch folder");
    let root = scratch.path().join("demo");
    init([OsString::from(format!("--data-dir={}", root.display()))]).expect("accepted");
    assert_eq!(heimdall_core::paths::data_root(), Some(root.as_path()));
    assert_eq!(
        titled("Heimdall".to_owned()),
        format!("Heimdall [data: {}]", root.display())
    );
    assert!(
        heimdall_core::paths::config_dir()
            .expect("a configuration folder")
            .starts_with(&root)
    );
}
