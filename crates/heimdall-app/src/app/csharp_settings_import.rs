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

//! What the migration of this computer's C# store carries over from its `settings.json`
//! besides the profiles: its preferences, offered for review as a settings file's import
//! shows them, and taken only once confirmed.

use heimdall_core::import::csharp_settings::{self, PreferenceOutcome, PreferenceRow};
use heimdall_core::settings::SettingsImport;

use super::{App, Dialog};

/// The preferences of the C# `settings.json`, as the import report says them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPreferences {
    /// What taking the preferences carried over changes here, shown before it is taken;
    /// `None` when they change nothing, or once opened.
    pub review: Option<Box<SettingsImport>>,
    /// How many preferences are carried over, to be reviewed.
    pub taken: usize,
    /// Each preference left out, by its C# name, and why: changed here, a secret, a value
    /// not read, or a path of the C# installation.
    pub left_out: Vec<PreferenceRow>,
}

impl ImportedPreferences {
    /// The C# names of the preferences left out for `outcome`.
    pub fn left_out(&self, outcome: PreferenceOutcome) -> impl Iterator<Item = &str> {
        self.left_out
            .iter()
            .filter(move |row| row.outcome == outcome)
            .map(|row| row.key.as_str())
    }
}

impl App {
    /// The preferences of the C# `settings.json` `text`, laid over these settings: none
    /// changed here, no secret; or why they could not be read.
    pub(super) fn csharp_preferences(&self, text: &str) -> Result<ImportedPreferences, String> {
        let read = csharp_settings::preferences(text, &self.settings)
            .map_err(|error| error.to_string())?;
        let taken = read.count(PreferenceOutcome::Taken);
        let review = self.settings.csharp_import(&read);
        Ok(ImportedPreferences {
            review: (!review.changes.is_empty()).then(|| Box::new(review)),
            taken,
            left_out: read
                .rows
                .into_iter()
                .filter(|row| row.outcome != PreferenceOutcome::Taken)
                .collect(),
        })
    }

    /// The import report's "Review": the preferences carried over shown as a settings
    /// file's import shows them, taken once confirmed. Nothing when there is none.
    pub(super) fn review_csharp_preferences(&mut self) {
        let review = match &mut self.dialog {
            Some(Dialog::ImportDone(summary)) => match &mut summary.preferences {
                Some(Ok(preferences)) => preferences.review.take(),
                _ => None,
            },
            _ => None,
        };
        if let Some(review) = review {
            self.dialog = Some(Dialog::ConfirmSettingsImport(review));
        }
    }
}
