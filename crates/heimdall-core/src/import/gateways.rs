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

//! The SSH gateways of an imported session file reconciled with those already saved, as the
//! C# `GatewayImportReconciler`: a gateway is the same as another when it logs in to the same
//! host (without case), port and user, whatever its identifier. The file's gateway is then
//! the one already there, and every imported profile going through it is rewired to it, so
//! that importing a colleague's file does not duplicate the bastions both use.

use std::collections::{HashMap, HashSet};

use super::csharp::ImportReport;
use crate::profile::{ProfileId, SshGateway};

/// What reconciling an import's gateways did, as the C# import counts it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// Gateways the import adds.
    pub created: usize,
    /// Gateways of the file found already saved, or earlier in the file: not added, the
    /// profiles going through them rewired.
    pub merged: usize,
}

/// What tells two gateways apart: the host without case and around spaces, the port, the user
/// without case, as the C# `GatewayIdentity`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Identity {
    host: String,
    port: u16,
    user: String,
}

impl Identity {
    fn of(gateway: &SshGateway) -> Self {
        let folded = |text: &str| text.trim().to_lowercase();
        Self {
            host: folded(&gateway.host),
            port: gateway.port,
            user: folded(gateway.username.as_deref().unwrap_or_default()),
        }
    }
}

/// Reconciles the gateways of `report` with `existing`, the gateways saved: a gateway of the
/// file that is one of them, or one earlier in the file, is left out and the profiles and
/// gateways going through it go through that one; one whose identifier is already taken by
/// another gateway is given a fresh one from `fresh_id`, as the C# gives a new GUID. A chain
/// of parents the rewiring closes on itself loses the parent that closes it.
pub fn reconcile(
    report: &mut ImportReport,
    existing: &[SshGateway],
    fresh_id: &mut dyn FnMut() -> ProfileId,
) -> Reconciliation {
    let mut saved: HashMap<Identity, ProfileId> = HashMap::new();
    for gateway in existing {
        saved
            .entry(Identity::of(gateway))
            .or_insert_with(|| gateway.id.clone());
    }
    let mut used: HashSet<ProfileId> = existing.iter().map(|gateway| gateway.id.clone()).collect();
    let mut batch: HashMap<Identity, ProfileId> = HashMap::new();
    let mut renamed: HashMap<ProfileId, ProfileId> = HashMap::new();
    let mut counts = Reconciliation::default();
    let mut kept = Vec::new();
    for mut gateway in std::mem::take(&mut report.gateways) {
        let identity = Identity::of(&gateway);
        if let Some(found) = saved.get(&identity).or_else(|| batch.get(&identity)) {
            renamed.insert(gateway.id, found.clone());
            counts.merged += 1;
            continue;
        }
        let mut id = gateway.id.clone();
        while used.contains(&id) {
            id = fresh_id();
        }
        used.insert(id.clone());
        batch.insert(identity, id.clone());
        renamed.insert(std::mem::replace(&mut gateway.id, id.clone()), id);
        kept.push(gateway);
    }
    let rewired = |gateway: &mut Option<ProfileId>| {
        if let Some(found) = gateway.as_ref().and_then(|id| renamed.get(id)) {
            *gateway = Some(found.clone());
        }
    };
    for gateway in &mut kept {
        rewired(&mut gateway.parent);
    }
    break_loops(&mut kept);
    for profile in &mut report.profiles {
        rewired(&mut profile.gateway);
    }
    for profile in &mut report.rdp {
        rewired(&mut profile.gateway);
    }
    for profile in &mut report.winrm {
        rewired(&mut profile.gateway);
    }
    counts.created = kept.len();
    report.gateways = kept;
    counts
}

/// Clears the parent of a gateway its parents lead back to, as the C# `BreakParentLoops`:
/// saved gateways never go through imported ones, so a loop runs through `added` alone.
fn break_loops(added: &mut [SshGateway]) {
    let mut parents: HashMap<ProfileId, ProfileId> = added
        .iter()
        .filter_map(|gateway| Some((gateway.id.clone(), gateway.parent.clone()?)))
        .collect();
    for gateway in added {
        let mut seen = HashSet::from([gateway.id.clone()]);
        let mut next = parents.get(&gateway.id).cloned();
        while let Some(parent) = next {
            if parent == gateway.id {
                // Cleared for the gateways after it too: the first found loses its parent.
                parents.remove(&gateway.id);
                gateway.parent = None;
                break;
            }
            next = parents.get(&parent).cloned();
            if !seen.insert(parent) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway(id: &str, host: &str, parent: Option<&str>) -> SshGateway {
        SshGateway {
            id: ProfileId::new(id),
            name: id.to_owned(),
            host: host.to_owned(),
            port: 22,
            username: Some("ops".to_owned()),
            key_path: None,
            parent: parent.map(ProfileId::new),
        }
    }

    #[test]
    fn two_gateways_of_the_file_one_through_the_other_and_the_same_lose_the_loop() {
        // "inner" goes through "outer", which is "inner" again by its address.
        let mut report = ImportReport {
            gateways: vec![
                gateway("inner", "bastion.lab", Some("outer")),
                gateway("outer", "BASTION.lab", None),
            ],
            ..ImportReport::default()
        };
        let counts = reconcile(&mut report, &[], &mut || ProfileId::new("fresh"));
        assert_eq!(
            counts,
            Reconciliation {
                created: 1,
                merged: 1
            }
        );
        assert_eq!(report.gateways.len(), 1);
        assert_eq!(report.gateways[0].id, ProfileId::new("inner"));
        assert_eq!(report.gateways[0].parent, None, "through itself no more");
    }
}
