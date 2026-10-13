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

//! The offer to migrate from the legacy PowerShell Heimdall at start, as the C#
//! `TryMigrateLegacyAsync` (`App.xaml.cs:1253-1371`), and the Settings page's "Offer legacy
//! migration at next startup" (`SettingsViewModel.cs:2466-2487`).
//!
//! Looked for once at start, when no profile is saved and the profiles and settings could be
//! read. Unlike the C#, which asks before its PIN and vault, the offer waits until the window
//! is free: after the PIN and the master password, so that only the user who opened the
//! application is asked, and the store it would write to is open. It names the folder found
//! and what it holds. Declined, it is not made again for the same files; accepted, the
//! profiles and the settings Heimdall-rs has are imported, and the result says what was left
//! out.

use std::path::PathBuf;

use heimdall_core::import::rdpmanager::LeftOut;
use heimdall_core::settings::LegacyMigration;

use super::{App, Dialog, Effect, ImportSummary, Notice};
use crate::rdpmanager::{OFFER_VERSION, Offer};

/// What is done about the migration from the legacy PowerShell Heimdall.
#[derive(Debug, Clone)]
pub enum LegacyMigrationMessage {
    /// What the look at start found.
    Found(Option<Box<Offer>>),
    /// The Settings page's "Offer legacy migration at next startup": the offer declined is
    /// forgotten.
    OfferAgain,
}

/// Where the migration stands in this run.
#[derive(Debug, Default)]
pub(super) struct LegacyMigrationState {
    /// The look at start was asked for.
    looked: bool,
    /// The offer found, waiting for the window to be free.
    pending: Option<Box<Offer>>,
}

/// What a migration did, as the C# `MigrationResult` with what an import of a C# file says
/// besides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyMigrationDone {
    /// Entries of the legacy servers examined.
    pub examined: usize,
    /// Profiles imported.
    pub imported: usize,
    /// Profiles left out, in the file's order.
    pub left_out: Vec<LeftOut>,
    /// Projects of the legacy settings, which Heimdall-rs has no equivalent for.
    pub projects: usize,
    /// Key paths left out, being no plain local path: their profiles and gateways were
    /// imported without them.
    pub key_paths_left_out: usize,
    /// The merge into the profiles: gateways, gateways left out, settings dropped.
    pub summary: ImportSummary,
    /// The settings taken were saved.
    pub settings_saved: bool,
}

impl LegacyMigrationDone {
    /// Whether something was left out, which the result says as a warning, as the C#
    /// `MigrationPresentationPolicy` does.
    #[must_use]
    pub fn partial(&self) -> bool {
        !self.left_out.is_empty() || !self.summary.skipped.is_empty() || !self.settings_saved
    }
}

impl App {
    /// The look for the legacy PowerShell Heimdall at start, from `start`, the program's
    /// folder: once, when no profile is saved and the profiles and settings could be read.
    pub fn look_for_legacy_installation(&mut self, start: Option<PathBuf>) -> Vec<Effect> {
        if std::mem::replace(&mut self.legacy_migration.looked, true)
            || !self.legacy_migration_possible()
        {
            return Vec::new();
        }
        start
            .map(|start| vec![Effect::FindLegacyInstallation(start)])
            .unwrap_or_default()
    }

    /// Whether a migration can be offered: nothing saved to lose, and the profiles and
    /// settings read from their files, neither started empty beside an unreadable one, which
    /// is then saved beside it under another name.
    fn legacy_migration_possible(&self) -> bool {
        let profiles = &self.config.profiles_file;
        self.store.path() == profiles.as_path()
            && self.settings_file == heimdall_core::settings::settings_path(profiles)
            && heimdall_core::export::session_count(&self.store) == 0
    }

    /// Applies a message about the migration.
    pub(super) fn legacy_migration_message(
        &mut self,
        message: LegacyMigrationMessage,
    ) -> Vec<Effect> {
        match message {
            LegacyMigrationMessage::Found(Some(offer)) => {
                if self
                    .settings
                    .legacy_migration
                    .should_offer(OFFER_VERSION, &offer.fingerprint)
                {
                    self.legacy_migration.pending = Some(offer);
                    self.offer_legacy_migration();
                } else {
                    log::info!("Legacy migration offer previously declined; not offered.");
                }
            }
            LegacyMigrationMessage::Found(None) => {}
            LegacyMigrationMessage::OfferAgain => self.offer_legacy_migration_again(),
        }
        Vec::new()
    }

    /// Shows the offer found once nothing else is asked: never over the PIN, the master
    /// password or another question. Dropped when a profile was saved meanwhile.
    pub fn offer_legacy_migration(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        let Some(offer) = self.legacy_migration.pending.take() else {
            return;
        };
        if self.legacy_migration_possible() {
            self.dialog = Some(Dialog::LegacyMigrationOffer(offer));
        }
    }

    /// The offer declined: not made again for the same files, as the C# `RecordDeclineAsync`.
    pub(super) fn decline_legacy_migration(&mut self, offer: &Offer) {
        self.settings.legacy_migration = LegacyMigration {
            declined_offer_version: OFFER_VERSION,
            declined_source_fingerprint: Some(offer.fingerprint.clone()),
        };
        if let Err(error) = self.settings.save(&self.settings_file) {
            log::error!("Could not persist the declined legacy migration offer: {error}");
        }
    }

    /// The offer accepted: the profiles and gateways merged as an import of a C# file, then
    /// the settings Heimdall-rs has laid over its own, and the result shown.
    pub(super) fn migrate_legacy(&mut self, offer: Offer) {
        let conversion = offer.conversion;
        let imported = conversion.imported();
        let Some(summary) = self.merge_import(conversion.report) else {
            log::error!("Legacy migration failed: the profiles could not be saved.");
            return;
        };
        conversion.settings.apply_to(&mut self.settings);
        let settings_saved = match self.settings.save(&self.settings_file) {
            Ok(()) => true,
            Err(error) => {
                log::error!("Legacy migration: the settings could not be saved: {error}");
                false
            }
        };
        log::info!(
            "Legacy migration: {} examined, {imported} imported, {} skipped, {} project(s) left out.",
            conversion.examined,
            conversion.left_out.len(),
            conversion.projects
        );
        self.dialog = Some(Dialog::LegacyMigrationDone(Box::new(LegacyMigrationDone {
            examined: conversion.examined,
            imported,
            left_out: conversion.left_out,
            projects: conversion.projects,
            key_paths_left_out: conversion.key_paths_left_out,
            summary,
            settings_saved,
        })));
    }

    /// "Offer legacy migration at next startup", as the C# `ClearDeclineAsync`: the offer
    /// declined is forgotten, and said; nothing starts now. Not saved, nothing changes.
    fn offer_legacy_migration_again(&mut self) {
        if !self.settings.legacy_migration.has_decline() {
            return;
        }
        let declined = std::mem::take(&mut self.settings.legacy_migration);
        match self.settings.save(&self.settings_file) {
            Ok(()) => self.tell(Notice::LegacyMigrationReoffered),
            Err(error) => {
                log::error!("Failed to clear the declined legacy migration offer: {error}");
                self.settings.legacy_migration = declined;
                self.tell(Notice::LegacyMigrationReofferFailed);
            }
        }
    }
}
