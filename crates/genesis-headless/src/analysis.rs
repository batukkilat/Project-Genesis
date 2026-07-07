//! Read-only structure diagnostics over canonical snapshots.
//!
//! Dev telemetry for the Phase 3 exit-criteria review: measures whether
//! multi-particle bonded components exist and persist across time. These are
//! graph facts (component counts, sizes, ages) — hypotheses about what a
//! structure might *be* belong to the Phase 5 Observer. Nothing here can
//! mutate simulation state: the input is a by-value snapshot.
//!
//! Everything is deterministic: components are reported in canonical order
//! (sorted by smallest member id) and matching across samples breaks ties by
//! that same order, so the same run always prints the same report.

use genesis_sim::snapshot::WorldSnapshot;
use std::collections::HashMap;

/// Connected components of the bond graph with at least two members.
/// Each component is sorted by id ascending; components are sorted by their
/// smallest member id.
pub fn bond_components(snap: &WorldSnapshot) -> Vec<Vec<u64>> {
    // Dense index for every id that appears in a bond. HashMap is lookup
    // only — iteration below walks the bonds slice, which is canonical.
    let mut index: HashMap<u64, usize> = HashMap::new();
    let mut ids: Vec<u64> = Vec::new();
    for b in &snap.bonds {
        for id in [b.a, b.b] {
            index.entry(id).or_insert_with(|| {
                ids.push(id);
                ids.len() - 1
            });
        }
    }

    // Union-find over the dense indices.
    let mut parent: Vec<usize> = (0..ids.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]]; // path halving
            i = parent[i];
        }
        i
    }
    for b in &snap.bonds {
        let (ra, rb) = (
            find(&mut parent, index[&b.a]),
            find(&mut parent, index[&b.b]),
        );
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }

    let mut groups: HashMap<usize, Vec<u64>> = HashMap::new();
    for (i, &id) in ids.iter().enumerate() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(id);
    }
    let mut components: Vec<Vec<u64>> = groups.into_values().collect();
    for c in &mut components {
        c.sort_unstable();
    }
    components.sort_unstable_by_key(|c| c[0]);
    components
}

/// Aggregate facts about one sampled snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampleStats {
    pub tick: u64,
    pub particles: usize,
    pub bonds: usize,
    pub components: usize,
    pub largest_component: usize,
    /// Members of components of size >= 3 — particles inside nontrivial
    /// structure, as opposed to isolated bonded pairs.
    pub in_multi: usize,
    pub total_matter: f64,
    pub total_energy: f64,
    pub total_information: f64,
}

pub fn sample_stats(snap: &WorldSnapshot, components: &[Vec<u64>]) -> SampleStats {
    SampleStats {
        tick: snap.tick,
        particles: snap.particles.len(),
        bonds: snap.bonds.len(),
        components: components.len(),
        largest_component: components.iter().map(Vec::len).max().unwrap_or(0),
        in_multi: components
            .iter()
            .filter(|c| c.len() >= 3)
            .map(Vec::len)
            .sum(),
        total_matter: snap.particles.iter().map(|p| p.matter as f64).sum(),
        total_energy: snap.particles.iter().map(|p| p.energy as f64).sum(),
        total_information: snap.particles.iter().map(|p| p.information as f64).sum(),
    }
}

/// A component being followed across samples.
#[derive(Debug, Clone)]
struct Tracked {
    members: Vec<u64>,
    /// Number of consecutive samples this component has been observed in.
    age: u32,
}

/// Summary of one tracker observation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackReport {
    /// Components matched to a component from the previous sample.
    pub continued: usize,
    /// Components with no predecessor this sample.
    pub born: usize,
    /// Previous components that found no successor.
    pub died: usize,
    /// Highest age among live components, in samples.
    pub oldest_age: u32,
    /// Live components at least `persist_after` samples old.
    pub persistent: usize,
}

/// Follows bond components across successive samples by member overlap.
///
/// A new component continues an old one when they share at least half the
/// members of the larger of the two — strict enough that a structure keeps
/// its identity through gradual churn but not through wholesale replacement.
/// Matching is deterministic: candidates are ranked by shared-member count,
/// ties broken by canonical order, and each old component is claimed once.
pub struct StructureTracker {
    persist_after: u32,
    tracked: Vec<Tracked>,
}

impl StructureTracker {
    /// `persist_after`: age in samples at which a component counts as
    /// persistent in reports.
    pub fn new(persist_after: u32) -> Self {
        StructureTracker {
            persist_after,
            tracked: Vec::new(),
        }
    }

    pub fn observe(&mut self, components: &[Vec<u64>]) -> TrackReport {
        // Membership map of the previous sample. Lookup only.
        let mut owner: HashMap<u64, usize> = HashMap::new();
        for (i, t) in self.tracked.iter().enumerate() {
            for &id in &t.members {
                owner.insert(id, i);
            }
        }

        let mut claimed = vec![false; self.tracked.len()];
        let mut next: Vec<Tracked> = Vec::with_capacity(components.len());
        let mut continued = 0usize;
        let mut born = 0usize;

        for comp in components {
            // Count shared members per previous component.
            let mut hits: HashMap<usize, usize> = HashMap::new();
            for id in comp {
                if let Some(&i) = owner.get(id) {
                    *hits.entry(i).or_default() += 1;
                }
            }
            // Deterministic best match: most shared members, then lowest
            // previous index (previous components are in canonical order).
            let mut candidates: Vec<(usize, usize)> = hits.into_iter().collect();
            candidates.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let matched = candidates.into_iter().find(|&(i, shared)| {
                !claimed[i] && 2 * shared >= comp.len().max(self.tracked[i].members.len())
            });
            let age = match matched {
                Some((i, _)) => {
                    claimed[i] = true;
                    continued += 1;
                    self.tracked[i].age + 1
                }
                None => {
                    born += 1;
                    1
                }
            };
            next.push(Tracked {
                members: comp.clone(),
                age,
            });
        }

        let died = claimed.iter().filter(|&&c| !c).count();
        self.tracked = next;
        TrackReport {
            continued,
            born,
            died,
            oldest_age: self.tracked.iter().map(|t| t.age).max().unwrap_or(0),
            persistent: self
                .tracked
                .iter()
                .filter(|t| t.age >= self.persist_after)
                .count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_sim::snapshot::{BondSnap, ParticleSnap, WorldSnapshot};

    fn snap(bonds: &[(u64, u64)]) -> WorldSnapshot {
        let mut ids: Vec<u64> = bonds.iter().flat_map(|&(a, b)| [a, b]).collect();
        ids.sort_unstable();
        ids.dedup();
        WorldSnapshot {
            tick: 0,
            rng_state: 0,
            rng_gamma: 0,
            next_id: ids.last().map_or(0, |&i| i + 1),
            stream_seed: 0,
            dt: 1.0 / 60.0,
            world_width: 64.0,
            world_height: 64.0,
            interaction_radius: 8.0,
            core_frac: 0.4,
            repulsion: 40.0,
            attraction: 5.0,
            bond_rest_length: 3.0,
            information_decay: 0.0,
            information_max: 1e30,
            lod: genesis_config::LodPolicy::default(),
            rules: Vec::new(),
            particles: ids
                .iter()
                .map(|&id| ParticleSnap {
                    id,
                    pos_x: 0.0,
                    pos_y: 0.0,
                    vel_x: 0.0,
                    vel_y: 0.0,
                    matter: 1.0,
                    energy: 0.5,
                    information: 0.25,
                })
                .collect(),
            bonds: bonds
                .iter()
                .map(|&(a, b)| BondSnap {
                    a,
                    b,
                    strength: 1.0,
                })
                .collect(),
        }
    }

    #[test]
    fn components_merge_shared_endpoints() {
        // 1-2-3 chain plus a separate 7-8 pair.
        let s = snap(&[(1, 2), (2, 3), (7, 8)]);
        let comps = bond_components(&s);
        assert_eq!(comps, vec![vec![1, 2, 3], vec![7, 8]]);
    }

    #[test]
    fn components_canonical_order_regardless_of_bond_order() {
        let a = bond_components(&snap(&[(1, 9), (2, 9), (4, 5)]));
        let b = bond_components(&snap(&[(4, 5), (2, 9), (1, 9)]));
        assert_eq!(a, b);
        assert_eq!(a, vec![vec![1, 2, 9], vec![4, 5]]);
    }

    #[test]
    fn no_bonds_no_components() {
        assert!(bond_components(&snap(&[])).is_empty());
    }

    #[test]
    fn stats_count_multi_membership_and_totals() {
        let s = snap(&[(1, 2), (2, 3), (7, 8)]);
        let comps = bond_components(&s);
        let stats = sample_stats(&s, &comps);
        assert_eq!(stats.particles, 5);
        assert_eq!(stats.bonds, 3);
        assert_eq!(stats.components, 2);
        assert_eq!(stats.largest_component, 3);
        assert_eq!(stats.in_multi, 3, "only the size-3 chain counts");
        assert_eq!(stats.total_matter, 5.0);
        assert_eq!(stats.total_energy, 2.5);
    }

    #[test]
    fn tracker_ages_stable_component() {
        let mut t = StructureTracker::new(3);
        let comps = vec![vec![1, 2, 3]];
        assert_eq!(t.observe(&comps).persistent, 0);
        assert_eq!(t.observe(&comps).persistent, 0);
        let r = t.observe(&comps);
        assert_eq!(r.oldest_age, 3);
        assert_eq!(r.persistent, 1);
        assert_eq!(r.continued, 1);
    }

    #[test]
    fn tracker_follows_through_member_churn() {
        let mut t = StructureTracker::new(2);
        t.observe(&[vec![1, 2, 3, 4]]);
        // Loses 4, gains 9: shares 3 of max(4, 4) members — still the same
        // structure.
        let r = t.observe(&[vec![1, 2, 3, 9]]);
        assert_eq!(r.continued, 1);
        assert_eq!(r.born, 0);
        assert_eq!(r.oldest_age, 2);
    }

    #[test]
    fn tracker_kills_wholesale_replacement() {
        let mut t = StructureTracker::new(2);
        t.observe(&[vec![1, 2, 3, 4]]);
        // Shares only 1 of 4 members — a different structure.
        let r = t.observe(&[vec![4, 10, 11, 12]]);
        assert_eq!(r.continued, 0);
        assert_eq!(r.born, 1);
        assert_eq!(r.died, 1);
        assert_eq!(r.oldest_age, 1);
    }

    #[test]
    fn tracker_claims_each_predecessor_once() {
        let mut t = StructureTracker::new(2);
        t.observe(&[vec![1, 2, 3, 4, 5, 6]]);
        // The old component split in half; only one half (the earlier one in
        // canonical order) may inherit the identity.
        let r = t.observe(&[vec![1, 2, 3], vec![4, 5, 6]]);
        assert_eq!(r.continued, 1);
        assert_eq!(r.born, 1);
        assert_eq!(r.died, 0);
    }

    #[test]
    fn tracker_reports_deaths() {
        let mut t = StructureTracker::new(2);
        t.observe(&[vec![1, 2], vec![5, 6]]);
        let r = t.observe(&[vec![1, 2]]);
        assert_eq!(r.died, 1);
        assert_eq!(r.continued, 1);
    }
}
