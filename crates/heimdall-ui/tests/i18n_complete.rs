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

use std::path::Path;

#[test]
fn every_supported_language_declares_the_same_keys() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let gaps = heimdall_i18n::find_gaps(crate_root, env!("CARGO_PKG_NAME"))
        .expect("every language file exists and parses");
    assert!(gaps.is_empty(), "keys differ between languages: {gaps:#?}");
}
