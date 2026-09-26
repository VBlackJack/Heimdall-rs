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

//! Fluent strings of the application, embedded in the binary.

use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::unic_langid::LanguageIdentifier;
use i18n_embed::{DesktopLanguageRequester, LanguageLoader};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

/// Loader holding the application strings, fallback language loaded.
pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("the fallback language is embedded at build time");
    // Values are plain text in a plain-text widget: bidi isolation marks would show as boxes.
    loader.set_use_isolating(false);
    loader
});

/// Looks a key up in [`LOADER`]; an unknown key fails the build.
macro_rules! fl {
    ($id:literal) => {{ i18n_embed_fl::fl!($crate::i18n::LOADER, $id) }};
    ($id:literal, $($args:tt)*) => {{ i18n_embed_fl::fl!($crate::i18n::LOADER, $id, $($args)*) }};
}

pub(crate) use fl;

/// Switches to the best language the desktop asks for.
pub fn init() {
    select(&LOADER, &DesktopLanguageRequester::requested_languages());
}

/// Switches `loader` to the best of `requested`. Selecting builds new bundles, which start
/// with isolation marks on whatever the loader was set to, so they are turned off again.
fn select(loader: &FluentLanguageLoader, requested: &[LanguageIdentifier]) {
    // On failure the fallback language, already loaded, stays active.
    let _ = i18n_embed::select(loader, &Localizations, requested);
    loader.set_use_isolating(false);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn a_selected_language_carries_no_isolation_marks() {
        // A loader of its own: the shared one would change language under other tests.
        let loader: FluentLanguageLoader = fluent_language_loader!();
        loader
            .load_fallback_language(&Localizations)
            .expect("fallback");
        select(&loader, &["fr".parse().expect("language tag")]);
        let text = loader.get_args("ui-files-size-kib", HashMap::from([("value", "1.0")]));
        assert_eq!(text, "1.0 Kio");
    }
}
