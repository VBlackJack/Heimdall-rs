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

//! Which gateways may be the parent of a gateway being added or edited, as the C#
//! `GatewayParentEligibility` decides: only a parent that leaves every chain running through
//! the gateway one a connection follows. Not the gateway itself nor one of its descendants,
//! which would close a loop; not a parent whose own route does not resolve; not one whose
//! route, plus the longest chain hanging below the gateway, is deeper than
//! [`MAX_GATEWAY_CHAIN_DEPTH`].

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use crate::profile::{ProfileId, SshGateway};

/// The longest chain a connection follows, counting every gateway from the one nearest to
/// this machine to the one the session uses, as the C# `GatewayChainResolver.DefaultMaxDepth`.
/// The parent picker reads it too, so that it only offers a parent a connection accepts.
pub const MAX_GATEWAY_CHAIN_DEPTH: usize = 5;

/// The gateways the parent picker offers for `editing` (`None` for a new gateway), in
/// inventory order: the eligible ones, and the parent the gateway already has, eligible or
/// not. As the C# `BuildOptions`: a picker that lost its current value would turn a rename
/// into a silent switch to a direct connection that skips the bastion.
#[must_use]
pub fn parent_options<'a>(
    gateways: &'a [SshGateway],
    editing: Option<&ProfileId>,
) -> Vec<&'a SshGateway> {
    let eligible: HashSet<&ProfileId> =
        eligible_parents(gateways, editing, MAX_GATEWAY_CHAIN_DEPTH)
            .into_iter()
            .map(|gateway| &gateway.id)
            .collect();
    let current = editing.and_then(|id| {
        gateways
            .iter()
            .find(|gateway| gateway.id == *id)
            .and_then(|gateway| gateway.parent.as_ref())
    });
    let mut offered = Vec::new();
    for gateway in gateways {
        let wanted = eligible.contains(&gateway.id) || Some(&gateway.id) == current;
        // The current parent once, should the file name it twice.
        if wanted
            && !offered
                .iter()
                .any(|seen: &&SshGateway| seen.id == gateway.id)
        {
            offered.push(gateway);
        }
    }
    offered
}

/// The gateways that may become the parent of `editing` (`None` for a new gateway), in
/// inventory order, for chains of at most `max_depth` gateways, as the C#
/// `EligibleParents`.
///
/// Two gateways under one identifier are never offered: which one a parent names cannot be
/// decided. The first still stands for the identifier when the routes of the others are
/// walked.
#[must_use]
pub fn eligible_parents<'a>(
    gateways: &'a [SshGateway],
    editing: Option<&ProfileId>,
    max_depth: usize,
) -> Vec<&'a SshGateway> {
    let mut by_id: HashMap<&ProfileId, &SshGateway> = HashMap::new();
    let mut duplicated: HashSet<&ProfileId> = HashSet::new();
    for gateway in gateways {
        // The first of each identifier, as the C# `TryAdd` keeps it.
        match by_id.entry(&gateway.id) {
            Entry::Occupied(_) => {
                duplicated.insert(&gateway.id);
            }
            Entry::Vacant(slot) => {
                slot.insert(gateway);
            }
        }
    }
    let mut children: HashMap<&ProfileId, Vec<&ProfileId>> = HashMap::new();
    for gateway in by_id.values() {
        if let Some(parent) = &gateway.parent {
            children.entry(parent).or_default().push(&gateway.id);
        }
    }

    let mut excluded: HashSet<&ProfileId> = HashSet::new();
    let mut subtree_height = 1;
    if let Some(id) = editing {
        collect_subtree(id, &children, &mut excluded);
        subtree_height = height(id, &children, &mut HashSet::new());
    }

    gateways
        .iter()
        .filter(|candidate| {
            !excluded.contains(&candidate.id) && !duplicated.contains(&candidate.id)
        })
        .filter(|candidate| {
            route_length(candidate, &by_id)
                .is_some_and(|length| length + subtree_height <= max_depth)
        })
        .collect()
}

/// Puts `id` and every gateway reached through it into `collected`.
fn collect_subtree<'a>(
    id: &'a ProfileId,
    children: &HashMap<&'a ProfileId, Vec<&'a ProfileId>>,
    collected: &mut HashSet<&'a ProfileId>,
) {
    let mut pending = vec![id];
    while let Some(current) = pending.pop() {
        if !collected.insert(current) {
            continue;
        }
        if let Some(below) = children.get(current) {
            pending.extend(below.iter().copied());
        }
    }
}

/// The gateways on the longest chain from `id` down to a gateway nothing goes through,
/// `id` included. A child already on the path is a loop on disk, which the new parent
/// breaks for this gateway, so it lengthens nothing.
fn height<'a>(
    id: &'a ProfileId,
    children: &HashMap<&'a ProfileId, Vec<&'a ProfileId>>,
    path: &mut HashSet<&'a ProfileId>,
) -> usize {
    if !path.insert(id) {
        return 0;
    }
    let tallest = children.get(id).map_or(0, |below| {
        below
            .iter()
            .map(|&child| height(child, children, path))
            .max()
            .unwrap_or(0)
    });
    path.remove(id);
    tallest + 1
}

/// The gateways from `candidate` up to the one nearest to this machine, `candidate`
/// included; `None` when that route does not resolve: a loop, or a parent that is gone.
fn route_length(candidate: &SshGateway, by_id: &HashMap<&ProfileId, &SshGateway>) -> Option<usize> {
    let mut visited: HashSet<&ProfileId> = HashSet::new();
    let mut current = candidate;
    loop {
        if !visited.insert(&current.id) {
            return None;
        }
        match &current.parent {
            None => return Some(visited.len()),
            Some(parent) => current = by_id.get(parent)?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway(id: &str, parent: Option<&str>) -> SshGateway {
        SshGateway {
            id: ProfileId::new(id),
            name: id.to_uppercase(),
            host: format!("{id}.lab"),
            port: 22,
            username: Some("ops".to_owned()),
            key_path: None,
            parent: parent.map(ProfileId::new),
        }
    }

    fn ids(gateways: &[&SshGateway]) -> Vec<String> {
        gateways
            .iter()
            .map(|gateway| gateway.id.as_str().to_owned())
            .collect()
    }

    fn eligible(gateways: &[SshGateway], editing: Option<&str>) -> Vec<String> {
        let editing = editing.map(ProfileId::new);
        ids(&eligible_parents(
            gateways,
            editing.as_ref(),
            MAX_GATEWAY_CHAIN_DEPTH,
        ))
    }

    #[test]
    fn neither_the_gateway_nor_its_descendants_are_offered() {
        // a <- b <- c, and d on its own.
        let gateways = [
            gateway("a", None),
            gateway("b", Some("a")),
            gateway("c", Some("b")),
            gateway("d", None),
        ];
        assert_eq!(eligible(&gateways, Some("a")), ["d"]);
        assert_eq!(eligible(&gateways, Some("b")), ["a", "d"]);
        assert_eq!(eligible(&gateways, Some("c")), ["a", "b", "d"]);
        assert_eq!(eligible(&gateways, None), ["a", "b", "c", "d"], "a new one");
    }

    #[test]
    fn a_parent_that_makes_a_chain_deeper_than_five_is_not_offered() {
        // g1 <- g2 <- g3 <- g4 <- g5: g5 is five deep.
        let mut gateways = vec![gateway("g1", None)];
        for level in 2..=MAX_GATEWAY_CHAIN_DEPTH {
            gateways.push(gateway(
                &format!("g{level}"),
                Some(&format!("g{}", level - 1)),
            ));
        }
        gateways.push(gateway("leaf", None));
        // A new gateway under g5 would be the sixth.
        assert_eq!(eligible(&gateways, None), ["g1", "g2", "g3", "g4", "leaf"]);
        // A gateway with a child of its own needs room for two.
        gateways.push(gateway("child", Some("leaf")));
        assert_eq!(eligible(&gateways, Some("leaf")), ["g1", "g2", "g3"]);
    }

    #[test]
    fn a_parent_whose_own_route_is_broken_or_a_duplicate_is_not_offered() {
        let gateways = [
            gateway("orphan", Some("gone")),
            gateway("one", Some("two")),
            gateway("two", Some("one")),
            gateway("twin", None),
            gateway("twin", None),
            gateway("fine", None),
        ];
        assert_eq!(eligible(&gateways, None), ["fine"]);
    }

    #[test]
    fn the_current_parent_is_kept_even_when_no_longer_eligible() {
        // b is reached through a, which is reached through b: a loop on disk.
        let gateways = [
            gateway("a", Some("b")),
            gateway("b", Some("a")),
            gateway("c", None),
        ];
        let editing = ProfileId::new("b");
        assert_eq!(ids(&parent_options(&gateways, Some(&editing))), ["a", "c"]);
        assert_eq!(
            eligible(&gateways, Some("b")),
            ["c"],
            "a descendant all the same"
        );
    }
}
