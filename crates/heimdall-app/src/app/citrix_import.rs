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
//! Unlike the C#, which appends every application again at each scan, an application
//! already saved, the same name on the same `StoreFront`, is kept and its launch line
//! refreshed. With the vault locked or no store for secrets, the applications come in
//! without their launch line, launched through their `StoreFront`, where the C# refuses the
//! import.

use std::collections::HashSet;

use heimdall_core::import::citrix_cache::{CacheScan, CacheWarning, CachedApp};
use heimdall_core::profile::{CitrixProfile, ProfileId};

use super::{App, Dialog, Effect};

/// What a Citrix import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitrixImportOutcome {
    /// Applications saved as new profiles.
    pub added: usize,
    /// Applications already saved, their launch line refreshed.
    pub refreshed: usize,
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
    pub(super) fn citrix_scanned(&mut self, scan: CacheScan) {
        self.dialog = Some(if scan.apps.is_empty() {
            Dialog::CitrixImportNothing {
                warnings: scan.warnings,
            }
        } else {
            Dialog::ConfirmCitrixImport(Box::new(scan))
        });
    }

    /// The applications of `scan` saved, and what was done said.
    pub(super) fn import_citrix(&mut self, scan: CacheScan) {
        let keep_lines = self.can_save_passwords();
        let mut added: Vec<CitrixProfile> = Vec::new();
        let mut refreshed: HashSet<ProfileId> = HashSet::new();
        let mut lines = Vec::new();
        for app in scan.apps {
            let saved = self
                .store
                .citrix_profiles()
                .iter()
                .find(|profile| same_app(profile, &app))
                .map(|profile| profile.id.clone());
            let id = if let Some(id) = saved {
                refreshed.insert(id.clone());
                id
            } else if let Some(profile) = added.iter().find(|profile| same_app(profile, &app)) {
                // The same application twice in the cache: the last line kept.
                profile.id.clone()
            } else {
                let profile = self.cached_profile(&app, &added);
                let id = profile.id.clone();
                added.push(profile);
                id
            };
            lines.push((id, app.launch_line));
        }
        let count = added.len();
        if let Err(error) = self.store.apply(|store| {
            store.merge_citrix(added);
        }) {
            self.dialog = Some(Dialog::save_failed(&error));
            return;
        }
        let mut without_launch_lines = !keep_lines;
        if keep_lines {
            for (id, line) in &lines {
                if let Err(error) = self.save_citrix_launch_line(id, line) {
                    log::warn!("a Citrix launch line could not be saved: {error}");
                    without_launch_lines = true;
                }
            }
        }
        self.dialog = Some(Dialog::CitrixImportDone(CitrixImportOutcome {
            added: count,
            refreshed: refreshed.len(),
            without_launch_lines,
            warnings: scan.warnings,
        }));
    }

    /// A new Citrix profile for `app`, as the C# `ToServerProfiles` makes one: named after
    /// it, filed under its category, on its `StoreFront` with single sign-on.
    fn cached_profile(&self, app: &CachedApp, added: &[CitrixProfile]) -> CitrixProfile {
        let id = loop {
            let id = self.fresh_id();
            if !added.iter().any(|profile| profile.id == id) {
                break id;
            }
        };
        CitrixProfile {
            id,
            name: app.name.clone(),
            group: app.group(),
            store_front_url: app.store_url.clone(),
            app_name: Some(app.name.clone()),
            ica_file: None,
            seamless: true,
            sso: true,
        }
    }
}

/// Whether `profile` is `app`: the same application name, on the same `StoreFront`, the
/// address compared without case.
fn same_app(profile: &CitrixProfile, app: &CachedApp) -> bool {
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
