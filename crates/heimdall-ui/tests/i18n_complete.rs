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

use heimdall_i18n::GapKind;

/// New text is written in English first and translated later, in batches: a key missing
/// from another language is a translation still to do, shown in English meanwhile. A key
/// English does not have is refused: it is dead, or a typo that would never be shown. So is
/// a translation naming other variables than the English: it would show an error in place
/// of a value, or leave the value out.
#[test]
fn no_language_holds_a_key_english_lacks_or_other_variables() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let gaps = heimdall_i18n::find_gaps(crate_root, env!("CARGO_PKG_NAME"))
        .expect("every language file exists and parses");
    let refused: Vec<_> = gaps
        .iter()
        .filter(|gap| gap.kind != GapKind::Missing)
        .collect();
    assert!(refused.is_empty(), "refused: {refused:#?}");
    let missing = gaps.len() - refused.len();
    if missing > 0 {
        eprintln!("translations still to do: {missing}");
    }
}
