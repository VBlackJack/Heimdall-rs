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

//! "Import Citrix Apps", as the C# `ImportCitrixAppsAsync`: Citrix Workspace's local cache
//! scanned off the window's thread, the number of applications asked about, then each
//! saved as a Citrix profile, its launch line kept in the vault as a password is, never in
//! the profiles file.
//!
//! As the C# `CitrixImportMerger` (v2026.102301), an import is idempotent: an application
//! already saved is found by its Citrix identifier (`-CitrixID`) and its launch metadata
//! refreshed in place, with its display name, folder, notes, tags and favorites kept and
//! nothing deleted. The result says how many were added, updated, or already up to date.
//! Where the C# appends an application with no identifier again at each scan, an
//! application saved before identifiers were kept, or with none in its line, is found by
//! its name on the same `StoreFront` instead, and given the identifier. With the vault
//! locked or no store for secrets, the applications come in without their launch line,
//! launched through their `StoreFront`, where the C# refuses the import.

use std::collections::HashSet;

use heimdall_core::import::citrix_cache::{CacheScan, CacheWarning, CachedApp};
use heimdall_core::profile::{CitrixProfile, ProfileId};

use super::{App, Dialog, Effect};

/// What a Citrix import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitrixImportOutcome {
    /// Applications saved as new profiles.
    pub added: usize,
    /// Applications already saved whose launch metadata (`StoreFront`, application name or
    /// Citrix identifier) changed and was refreshed.
    pub updated: usize,
    /// Applications already saved that were up to date: only their launch line refreshed.
    pub unchanged: usize,
    /// The launch lines could not be kept: the vault locked, or no store for secrets.
    pub without_launch_lines: bool,
    /// What the scan said.
    pub warnings: Vec<CacheWarning>,
}

impl App {
    /// "Import Citrix Apps": the cache scanned off the window's thread.
    pub(super) fn scan_citrix() -> Vec<Effect> {
        vec![Effect::ScanCitrixCache]
    }

    /// The cache scanned: its applications asked about, or why there are none.
    pub(super) fn citrix_scanned(&mut self, mut scan: CacheScan) {
        // A saved setting that failed its check is ignored: said with the rest.
        scan.warnings.extend(
            self.settings
                .citrix_ignored
                .iter()
                .map(|setting| CacheWarning::SettingIgnored(*setting)),
        );
        self.dialog = Some(if scan.apps.is_empty() {
            Dialog::CitrixImportNothing {
                warnings: scan.warnings,
            }
        } else {
            Dialog::ConfirmCitrixImport(Box::new(scan))
        });
    }

    /// The applications of `scan` saved or refreshed, and what was done said.
    pub(super) fn import_citrix(&mut self, scan: CacheScan) {
        let keep_lines = self.can_save_passwords();
        let saved: Vec<CitrixProfile> = self.store.citrix_profiles().to_vec();
        // Every profile, the saved ones refreshed in place and the new ones after them.
        let mut profiles = saved.clone();
        let mut lines = Vec::new();
        for app in scan.apps {
            let id = if let Some(profile) = profiles.iter_mut().find(|p| same_app(p, &app)) {
                refresh(profile, &app);
                profile.id.clone()
            } else {
                let profile = self.cached_profile(&app, &profiles);
                let id = profile.id.clone();
                profiles.push(profile);
                id
            };
            // The same application twice in the cache: the last line kept.
            lines.push((id, app.launch_line));
        }
        let mut outcome = CitrixImportOutcome {
            added: 0,
            updated: 0,
            unchanged: 0,
            without_launch_lines: !keep_lines,
            warnings: scan.warnings,
        };
        let mut touched: HashSet<&ProfileId> = HashSet::new();
        let mut to_merge: Vec<CitrixProfile> = Vec::new();
        for (id, _) in &lines {
            if !touched.insert(id) {
                continue;
            }
            let Some(now) = profiles.iter().find(|p| &p.id == id) else {
                continue;
            };
            match saved.iter().find(|p| &p.id == id) {
                None => {
                    outcome.added += 1;
                    to_merge.push(now.clone());
                }
                Some(before) if before != now => {
                    outcome.updated += 1;
                    to_merge.push(now.clone());
                }
                Some(_) => outcome.unchanged += 1,
            }
        }
        if let Err(error) = self.store.apply(|store| {
            store.merge_citrix(to_merge);
        }) {
            self.dialog = Some(Dialog::save_failed(&error));
            return;
        }
        if keep_lines {
            for (id, line) in &lines {
                if let Err(error) = self.save_citrix_launch_line(id, line) {
                    log::warn!("a Citrix launch line could not be saved: {error}");
                    outcome.without_launch_lines = true;
                }
            }
        }
        self.dialog = Some(Dialog::CitrixImportDone(outcome));
    }

    /// A new Citrix profile for `app`, as the C# `ToServerProfiles` makes one: named after
    /// it, filed under its category, on its `StoreFront` with single sign-on.
    fn cached_profile(&self, app: &CachedApp, profiles: &[CitrixProfile]) -> CitrixProfile {
        let id = loop {
            let id = self.fresh_id();
            if !profiles.iter().any(|profile| profile.id == id) {
                break id;
            }
        };
        CitrixProfile {
            remote_os: heimdall_core::profile::RemoteOs::default(),
            id,
            name: app.name.clone(),
            group: app.group(),
            store_front_url: app.store_url.clone(),
            app_name: Some(app.name.clone()),
            ica_file: None,
            seamless: true,
            sso: true,
            citrix_id: app.citrix_id.clone(),
        }
    }
}

/// Refreshes the launch metadata of `profile` from `app`, as the C# merger does: the
/// `StoreFront` (when the cache gives one), the application name and the Citrix identifier.
/// Its name, folder and every other field stay as the user left them.
fn refresh(profile: &mut CitrixProfile, app: &CachedApp) {
    if app.store_url.is_some() {
        profile.store_front_url.clone_from(&app.store_url);
    }
    profile.app_name = Some(app.name.clone());
    if app.citrix_id.is_some() {
        profile.citrix_id.clone_from(&app.citrix_id);
    }
}

/// Whether `profile` is `app`: the same Citrix identifier when both have one, as the C#
/// matches them; else the same application name on the same `StoreFront`, the address
/// compared without case.
fn same_app(profile: &CitrixProfile, app: &CachedApp) -> bool {
    if let (Some(saved), Some(scanned)) = (profile.citrix_id.as_deref(), app.citrix_id.as_deref()) {
        return saved == scanned;
    }
    let same_store = match (
        non_blank(profile.store_front_url.as_deref()),
        app.store_url.as_deref(),
    ) {
        (Some(saved), Some(scanned)) => saved.eq_ignore_ascii_case(scanned),
        (None, None) => true,
        _ => false,
    };
    same_store && non_blank(profile.app_name.as_deref()) == Some(app.name.as_str())
}

/// `text` trimmed, `None` when blank.
fn non_blank(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|text| !text.is_empty())
}
