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

//! The migration from the legacy PowerShell Heimdall in the window, as the C# says it: the
//! offer at start, with the folder found and what it holds, then what the migration did, as
//! the C# `MigrationPresentationPolicy`.

use heimdall_app::rdpmanager::Offer;
use heimdall_app::{LegacyMigrationDone, server_text};
use heimdall_core::import::rdpmanager::{LeftOut, LeftOutBecause};

use crate::i18n::fl;
use crate::texts;

/// Profiles left out named at most, as the C# `MaxDisplayedWarnings`.
pub const MAX_LEFT_OUT_SHOWN: usize = 5;

/// The title, question, decline and action of the offer `offer`.
#[must_use]
pub fn offer(offer: &Offer) -> (String, String, String, String) {
    (
        fl!("ui-legacy-migration-title"),
        fl!(
            "ui-legacy-migration-offer",
            path = offer.source.display().to_string(),
            profiles = offer.conversion.examined,
            gateways = offer.conversion.gateways
        ),
        fl!("ui-legacy-migration-decline"),
        fl!("ui-legacy-migration-import"),
    )
}

/// The title and lines of what the migration `done` did, and whether they warn: something
/// left out, or the settings not saved.
#[must_use]
pub fn done(done: &LegacyMigrationDone) -> (String, Vec<String>, bool) {
    let mut lines = Vec::new();
    if done.left_out.is_empty() {
        lines.push(fl!("ui-legacy-migration-success", count = done.imported));
    } else {
        lines.push(fl!(
            "ui-legacy-migration-partial",
            examined = done.examined,
            imported = done.imported,
            skipped = done.left_out.len()
        ));
        lines.extend(done.left_out.iter().take(MAX_LEFT_OUT_SHOWN).map(left_out));
        let omitted = done.left_out.len().saturating_sub(MAX_LEFT_OUT_SHOWN);
        if omitted > 0 {
            lines.push(fl!("ui-legacy-migration-omitted", count = omitted));
        }
    }
    let gateways = done.summary.gateways;
    if gateways.created + gateways.merged + gateways.orphans > 0 {
        lines.push(fl!(
            "ui-dialog-import-gateways",
            created = gateways.created,
            merged = gateways.merged,
            orphans = gateways.orphans
        ));
    }
    lines.extend(done.summary.skipped.iter().map(|(name, reason)| {
        fl!(
            "ui-dialog-import-skipped-item",
            name = server_text(name),
            reason = texts::skip_reason(reason)
        )
    }));
    if !done.summary.dropped.is_empty() {
        lines.push(fl!("ui-dialog-import-dropped"));
        lines.extend(done.summary.dropped.iter().map(|(name, settings)| {
            let settings: Vec<String> = settings
                .iter()
                .map(|setting| texts::dropped_setting(*setting))
                .collect();
            fl!(
                "ui-dialog-import-dropped-item",
                name = name.as_str(),
                settings = settings.join(&fl!("ui-dialog-import-dropped-separator"))
            )
        }));
    }
    if done.projects > 0 {
        lines.push(fl!("ui-legacy-migration-projects", count = done.projects));
    }
    lines.push(fl!("ui-legacy-migration-secrets"));
    if done.key_paths_left_out > 0 {
        lines.push(fl!(
            "ui-legacy-migration-key-paths",
            count = done.key_paths_left_out
        ));
    }
    if !done.settings_saved {
        lines.push(fl!("ui-legacy-migration-settings-not-saved"));
    }
    (fl!("ui-legacy-migration-title"), lines, done.partial())
}

/// A profile left out, as the C# `MigrationPartialItem`: its place, its name, why.
fn left_out(left: &LeftOut) -> String {
    let name = left
        .name
        .as_deref()
        .map_or_else(|| fl!("ui-legacy-migration-unnamed"), server_text);
    let reason = match &left.reason {
        LeftOutBecause::InvalidLegacyField => fl!("ui-legacy-migration-invalid-field"),
        LeftOutBecause::Refused(reason) => texts::skip_reason(reason),
    };
    fl!(
        "ui-legacy-migration-partial-item",
        index = left.index,
        name = name,
        reason = reason
    )
}
