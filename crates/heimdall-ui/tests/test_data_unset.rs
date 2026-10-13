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

//! A test build whose scratch folder was never set: it refuses to start, and nothing it
//! would read is a real folder. Its own process, where no folder is ever set.

#![cfg(feature = "test-data-dir")]

use heimdall_ui::test_data::{ArgumentError, ensure_set, titled};

#[test]
fn without_its_folder_a_test_build_refuses_to_start_and_knows_no_real_folder() {
    assert_eq!(ensure_set(), Err(ArgumentError::NotSet));
    assert_eq!(heimdall_core::paths::config_dir(), None);
    assert_eq!(heimdall_core::paths::data_dir(), None);
    assert_eq!(heimdall_core::paths::log_dir(), None);
    assert_eq!(heimdall_core::paths::home_dir(), None);
    assert_eq!(heimdall_core::paths::temp_dir(), None);
    assert_ne!(
        heimdall_core::paths::credential_service(heimdall_core::paths::APPLICATION),
        heimdall_core::paths::APPLICATION
    );
    assert_eq!(titled("Heimdall".to_owned()), "Heimdall");
}

#[test]
fn the_start_checks_the_folder_before_anything_else() {
    // `run` cannot be called here, it would exit: its first statement is the check.
    let lib = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("lib.rs"),
    )
    .expect("lib.rs");
    let start = lib.find("pub fn run() -> iced::Result {").expect("run");
    let body = &lib[start..];
    let check = body.find("test_data::ensure_set()").expect("the check");
    let first_read = body.find("paths::").expect("a folder read");
    let restrict = body.find("folder_acl").expect("the folders restricted");
    assert!(check < first_read && check < restrict);
}
