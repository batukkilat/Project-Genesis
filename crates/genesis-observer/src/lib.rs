//! Layer 5: the Observer — analyzes the simulation without affecting it.
//!
//! Phase 5 starts here (docs/research/observer-design.md): this crate began
//! as the headless CLI's structure diagnostics, promoted to the Observer
//! layer. It consumes read-only [`WorldSnapshot`]s and extracts graph facts —
//! bonded components, sample stats, and structure identity across time.
//! Metrics and confidence-scored hypotheses build on these (landing steps
//! 2-4 in the design doc).
//!
//! The constitution's read-only guarantee is a type-system fact: nothing in
//! this crate receives a mutable reference to any simulation type, so
//! running the Observer cannot change a single simulated bit (proven by the
//! replay-compatibility test).
//!
//! Everything is deterministic: components are reported in canonical order
//! (sorted by smallest member id) and matching across samples breaks ties by
//! that same order, so the same run always yields the same observations.

use genesis_config::ConfigError;
use genesis_sim::snapshot::WorldSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Observer configuration: thresholds for structure identity and reporting.
///
/// Deliberately NOT part of replay identity — the Observer cannot affect the
/// simulation, so two runs differing only in observer config are the same
/// universe by construction. Changing these values changes what gets
/// *reported*, never what *happens*.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObserverConfig {
    /// Fraction of the larger component's members that must be shared for a
    /// new component to continue an old one's identity (closed bound:
    /// `shared >= overlap * larger`). In (0, 1]. Higher is stricter: 1.0
    /// demands identical membership; low values let identities survive
    /// heavy churn.
    pub overlap: f32,
    /// Age in consecutive samples at which a structure counts as persistent
    /// in reports.
    pub persist_after: u32,
}

impl Default for ObserverConfig {
    fn default() -> Self {
        ObserverConfig {
            overlap: 0.5,
            persist_after: 5,
        }
    }
}

impl ObserverConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let config: ObserverConfig =
            ron::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(self.overlap > 0.0 && self.overlap <= 1.0) {
            return Err(ConfigError::Invalid(format!(
                "observer overlap must be in (0, 1], got {}",
                self.overlap
            )));
        }
        Ok(())
    }
}

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
#[derive(Debug, Clone, PartialEq)]
pub struct TrackedStructure {
    /// Stable observer-side identity: assigned when the structure is first
    /// seen, kept while it matches across samples, and never reused — so
    /// metrics and the timeline can reference "structure 17" across its
    /// whole life. Observer ids are unrelated to particle ids.
    pub id: u64,
    /// Member particle ids, sorted ascending.
    pub members: Vec<u64>,
    /// Number of consecutive samples this component has been observed in.
    pub age: u32,
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
/// A new component continues an old one when they share at least
/// `config.overlap` of the members of the larger of the two (default: half)
/// — strict enough that a structure keeps its identity through gradual churn
/// but not through wholesale replacement. Matching is deterministic:
/// candidates are ranked by shared-member count, ties broken by canonical
/// order, and each old component is claimed once. A continued component
/// keeps its stable observer id; ids are never reused.
pub struct StructureTracker {
    config: ObserverConfig,
    tracked: Vec<TrackedStructure>,
    /// Next observer id to assign. Ids start at 1 and never repeat within a
    /// tracker's lifetime, even after the structure that held one dies.
    next_id: u64,
}

impl StructureTracker {
    pub fn new(config: ObserverConfig) -> Self {
        StructureTracker {
            config,
            tracked: Vec::new(),
            next_id: 1,
        }
    }

    /// Structures live as of the last `observe` call, in canonical order
    /// (sorted by smallest member id, mirroring [`bond_components`]).
    pub fn structures(&self) -> &[TrackedStructure] {
        &self.tracked
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
        let mut next: Vec<TrackedStructure> = Vec::with_capacity(components.len());
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
                let larger = comp.len().max(self.tracked[i].members.len());
                !claimed[i] && shared as f64 >= self.config.overlap as f64 * larger as f64
            });
            let (id, age) = match matched {
                Some((i, _)) => {
                    claimed[i] = true;
                    continued += 1;
                    (self.tracked[i].id, self.tracked[i].age + 1)
                }
                None => {
                    born += 1;
                    let id = self.next_id;
                    self.next_id += 1;
                    (id, 1)
                }
            };
            next.push(TrackedStructure {
                id,
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
                .filter(|t| t.age >= self.config.persist_after)
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
            env_cols: 0,
            env_rows: 0,
            env_fields: Vec::new(),
            env_dynamics: Vec::new(),
            pending_actions: Vec::new(),
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

    /// Tracker with the given overlap threshold and persistence age.
    fn tracker(overlap: f32, persist_after: u32) -> StructureTracker {
        StructureTracker::new(ObserverConfig {
            overlap,
            persist_after,
        })
    }

    #[test]
    fn tracker_ages_stable_component() {
        let mut t = tracker(0.5, 3);
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
        let mut t = tracker(0.5, 2);
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
        let mut t = tracker(0.5, 2);
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
        let mut t = tracker(0.5, 2);
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
        let mut t = tracker(0.5, 2);
        t.observe(&[vec![1, 2], vec![5, 6]]);
        let r = t.observe(&[vec![1, 2]]);
        assert_eq!(r.died, 1);
        assert_eq!(r.continued, 1);
    }

    #[test]
    fn structure_keeps_id_through_churn() {
        let mut t = tracker(0.5, 2);
        t.observe(&[vec![1, 2, 3, 4]]);
        let id = t.structures()[0].id;
        // Gradual full turnover: each step shares 3 of 4 members, so the
        // identity survives even though no original member remains.
        t.observe(&[vec![1, 2, 3, 9]]);
        t.observe(&[vec![2, 3, 9, 10]]);
        t.observe(&[vec![3, 9, 10, 11]]);
        t.observe(&[vec![9, 10, 11, 12]]);
        let s = &t.structures()[0];
        assert_eq!(s.id, id);
        assert_eq!(s.age, 5);
        assert_eq!(s.members, vec![9, 10, 11, 12]);
    }

    #[test]
    fn observer_ids_never_reused() {
        let mut t = tracker(0.5, 2);
        t.observe(&[vec![1, 2]]);
        let first = t.structures()[0].id;
        // The structure dies; an unrelated one is born and must get a fresh
        // id even though the slot freed up.
        t.observe(&[vec![7, 8]]);
        let second = t.structures()[0].id;
        assert_ne!(first, second);
        // And a third after another wholesale replacement.
        t.observe(&[vec![20, 21]]);
        let third = t.structures()[0].id;
        assert_ne!(second, third);
        assert_ne!(first, third);
    }

    #[test]
    fn overlap_threshold_is_configurable() {
        // 2 shared of larger=4 is exactly half: continues at 0.5 ...
        let mut loose = tracker(0.5, 2);
        loose.observe(&[vec![1, 2, 3, 4]]);
        assert_eq!(loose.observe(&[vec![1, 2, 8, 9]]).continued, 1);
        // ... but not at a stricter 0.75.
        let mut strict = tracker(0.75, 2);
        strict.observe(&[vec![1, 2, 3, 4]]);
        let r = strict.observe(&[vec![1, 2, 8, 9]]);
        assert_eq!(r.continued, 0);
        assert_eq!(r.born, 1);
        assert_eq!(r.died, 1);
    }

    #[test]
    fn overlap_one_requires_identical_membership() {
        let mut t = tracker(1.0, 2);
        t.observe(&[vec![1, 2, 3]]);
        assert_eq!(t.observe(&[vec![1, 2, 3]]).continued, 1);
        assert_eq!(t.observe(&[vec![1, 2, 3, 4]]).continued, 0);
    }

    #[test]
    fn config_default_matches_previous_hardcoded_behavior() {
        let c = ObserverConfig::default();
        assert_eq!(c.overlap, 0.5);
        assert_eq!(c.persist_after, 5);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn config_rejects_out_of_range_overlap() {
        for bad in [0.0, -0.5, 1.5, f32::NAN] {
            let c = ObserverConfig {
                overlap: bad,
                ..ObserverConfig::default()
            };
            assert!(c.validate().is_err(), "overlap {bad} must be rejected");
        }
    }

    #[test]
    fn config_ron_roundtrip_and_partial_defaults() {
        let c = ObserverConfig {
            overlap: 0.7,
            persist_after: 9,
        };
        let text = ron::ser::to_string(&c).unwrap();
        let back: ObserverConfig = ron::from_str(&text).unwrap();
        assert_eq!(c, back);
        // Omitted fields fall back to defaults, matching SimConfig behavior.
        let partial: ObserverConfig = ron::from_str("(overlap: 0.25)").unwrap();
        assert_eq!(partial.overlap, 0.25);
        assert_eq!(
            partial.persist_after,
            ObserverConfig::default().persist_after
        );
    }
}
