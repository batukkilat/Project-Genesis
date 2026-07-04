//! Layer 1: the Interaction System — discrete, rule-driven state transitions.
//!
//! Phase 3 skeleton: quantity transfers between nearby particles, following
//! the constitution pipeline (condition → probability → action) with a
//! deterministic two-phase collect-then-commit step. Costs, bonds, and
//! create/destroy join once their parked design questions (QUESTIONS.md) are
//! answered; the machinery here is what they will plug into.
//!
//! Rules here are the *compiled internal representation*. How rules are
//! authored on disk is parked (QUESTIONS.md Q2) — whatever the answer, it
//! compiles down to these structs.
//!
//! Determinism:
//! - Phase A (parallel collect): every candidate pair rolls its own RNG
//!   stream named `derive(stream_seed, [tick, id_i, id_j, rule])` — no
//!   shared stream state, so evaluation order and thread count are
//!   irrelevant. Intents land in per-chunk vectors whose concatenation
//!   order is fixed by the canonical particle layout.
//! - Phase B (sequential commit): intents apply in that canonical order,
//!   clamped against current stocks. Single-threaded by design — commit
//!   cost is proportional to events, not particles.

use bevy_ecs::prelude::*;
use genesis_core::{DetRng, StateHasher};
use rayon::prelude::*;

use crate::grid::GridGeom;
use crate::store::{ParticleStore, par_chunk};

/// Matter floor left on a donor: matter is inertial mass and must stay
/// positive (integration divides by it).
pub const MIN_MATTER: f32 = 1e-6;

/// Closed interval used by rule conditions. `ANY` matches everything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: f32,
    pub max: f32,
}

impl Bounds {
    pub const ANY: Bounds = Bounds {
        min: f32::NEG_INFINITY,
        max: f32::INFINITY,
    };

    pub fn contains(&self, v: f32) -> bool {
        v >= self.min && v <= self.max
    }
}

/// Conditions on one particle's fundamental quantities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantityCondition {
    pub matter: Bounds,
    pub energy: Bounds,
    pub information: Bounds,
}

impl QuantityCondition {
    pub const ANY: QuantityCondition = QuantityCondition {
        matter: Bounds::ANY,
        energy: Bounds::ANY,
        information: Bounds::ANY,
    };

    pub fn matches(&self, matter: f32, energy: f32, information: f32) -> bool {
        self.matter.contains(matter)
            && self.energy.contains(energy)
            && self.information.contains(information)
    }
}

/// One compiled interaction rule: an ordered pair event from an initiator to
/// an other particle within `radius`, firing with `probability` per candidate
/// pair per tick. Note both orderings of a pair are evaluated independently.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompiledRule {
    pub radius: f32,
    pub self_cond: QuantityCondition,
    pub other_cond: QuantityCondition,
    pub probability: f32,
    /// Amounts moved initiator → other, clamped to the donor's stock at
    /// commit time (matter additionally keeps `MIN_MATTER` on the donor).
    pub transfer_matter: f32,
    pub transfer_energy: f32,
    pub transfer_information: f32,
}

/// The active rule set. Part of replay identity — its content is hashed into
/// the state hash and serialized into saves.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct RuleSet {
    pub rules: Vec<CompiledRule>,
}

impl RuleSet {
    pub fn hash_into(&self, h: &mut StateHasher) {
        h.write_u64(self.rules.len() as u64);
        for r in &self.rules {
            for v in r.fields() {
                h.write_f32(v);
            }
        }
    }

    /// Panics if any rule is malformed. Runs once at simulation assembly so a
    /// bad rule can never reach the hot loop, where a NaN transfer would
    /// silently drain stocks (`NaN.min(x) == x`) and an oversized radius
    /// would silently under-fire (the pair scan only covers the 3x3 grid
    /// block, whose cell size is the physics interaction radius).
    pub fn assert_valid(&self, max_radius: f32) {
        for (i, r) in self.rules.iter().enumerate() {
            for v in r.fields() {
                assert!(!v.is_nan(), "rule {i}: NaN field");
            }
            assert!(
                r.radius > 0.0 && r.radius <= max_radius,
                "rule {i}: radius {} outside (0, {max_radius}] — pairs beyond one grid cell are never scanned",
                r.radius
            );
            assert!(
                (0.0..=1.0).contains(&r.probability),
                "rule {i}: probability {} outside [0, 1]",
                r.probability
            );
            assert!(
                r.transfer_matter >= 0.0
                    && r.transfer_energy >= 0.0
                    && r.transfer_information >= 0.0,
                "rule {i}: transfers must be non-negative"
            );
        }
    }
}

impl CompiledRule {
    /// Canonical field order, used by hashing and serialization. Keep in
    /// sync with `from_fields`.
    pub fn fields(&self) -> [f32; 17] {
        [
            self.radius,
            self.self_cond.matter.min,
            self.self_cond.matter.max,
            self.self_cond.energy.min,
            self.self_cond.energy.max,
            self.self_cond.information.min,
            self.self_cond.information.max,
            self.other_cond.matter.min,
            self.other_cond.matter.max,
            self.other_cond.energy.min,
            self.other_cond.energy.max,
            self.other_cond.information.min,
            self.other_cond.information.max,
            self.probability,
            self.transfer_matter,
            self.transfer_energy,
            self.transfer_information,
        ]
    }

    pub fn from_fields(f: [f32; 17]) -> Self {
        CompiledRule {
            radius: f[0],
            self_cond: QuantityCondition {
                matter: Bounds {
                    min: f[1],
                    max: f[2],
                },
                energy: Bounds {
                    min: f[3],
                    max: f[4],
                },
                information: Bounds {
                    min: f[5],
                    max: f[6],
                },
            },
            other_cond: QuantityCondition {
                matter: Bounds {
                    min: f[7],
                    max: f[8],
                },
                energy: Bounds {
                    min: f[9],
                    max: f[10],
                },
                information: Bounds {
                    min: f[11],
                    max: f[12],
                },
            },
            probability: f[13],
            transfer_matter: f[14],
            transfer_energy: f[15],
            transfer_information: f[16],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Intent {
    initiator: u32,
    other: u32,
    rule: u32,
}

/// Run one interaction step: collect intents in parallel, commit sequentially
/// in canonical order. Store must be canonicalized and positions must not
/// have moved since (call between `forces` and `integrate`).
pub fn apply(
    store: &mut ParticleStore,
    geom: &GridGeom,
    rules: &RuleSet,
    stream_seed: u64,
    tick: u64,
) {
    if rules.rules.is_empty() || store.is_empty() {
        return;
    }

    // Phase A: parallel collect. Chunk partition follows the canonical
    // layout, so the concatenation order below is state-pure.
    let n = store.len();
    let px: &[f32] = &store.px;
    let py: &[f32] = &store.py;
    let id: &[u64] = &store.id;
    let matter: &[f32] = &store.matter;
    let energy: &[f32] = &store.energy;
    let information: &[f32] = &store.information;
    let cell: &[u32] = &store.cell;
    let cell_start: &[u32] = &store.cell_start;
    let world = (geom.world_w, geom.world_h);

    let n_chunks = n.div_ceil(par_chunk());
    let intent_chunks: Vec<Vec<Intent>> = (0..n_chunks)
        .into_par_iter()
        .map(|c| {
            let lo = c * par_chunk();
            let hi = (lo + par_chunk()).min(n);
            let mut out = Vec::new();
            for i in lo..hi {
                for (ridx, rule) in rules.rules.iter().enumerate() {
                    if !rule.self_cond.matches(matter[i], energy[i], information[i]) {
                        continue;
                    }
                    let r2_cut = rule.radius * rule.radius;
                    for &nc in geom.neighbors_of(cell[i]).iter() {
                        let start = cell_start[nc as usize] as usize;
                        let end = cell_start[nc as usize + 1] as usize;
                        for j in start..end {
                            if j == i {
                                continue;
                            }
                            let dx = genesis_core::torus::delta(px[i], px[j], world.0);
                            let dy = genesis_core::torus::delta(py[i], py[j], world.1);
                            if dx * dx + dy * dy >= r2_cut {
                                continue;
                            }
                            if !rule
                                .other_cond
                                .matches(matter[j], energy[j], information[j])
                            {
                                continue;
                            }
                            let roll =
                                DetRng::derive(stream_seed, &[tick, id[i], id[j], ridx as u64])
                                    .next_f32();
                            if roll < rule.probability {
                                out.push(Intent {
                                    initiator: i as u32,
                                    other: j as u32,
                                    rule: ridx as u32,
                                });
                            }
                        }
                    }
                }
            }
            out
        })
        .collect();

    // Phase B: sequential commit in canonical order. Conditions were checked
    // against tick-start state; amounts clamp against *current* stocks so
    // earlier events in the same tick can starve later ones — deterministic,
    // and conservation holds regardless.
    let mut committed = 0u64;
    for intent in intent_chunks.iter().flatten() {
        let rule = &rules.rules[intent.rule as usize];
        let i = intent.initiator as usize;
        let j = intent.other as usize;

        let m = rule
            .transfer_matter
            .min((store.matter[i] - MIN_MATTER).max(0.0))
            .max(0.0);
        let e = rule.transfer_energy.min(store.energy[i]).max(0.0);
        let inf = rule.transfer_information.min(store.information[i]).max(0.0);

        store.matter[i] -= m;
        store.matter[j] += m;
        store.energy[i] -= e;
        store.energy[j] += e;
        store.information[i] -= inf;
        store.information[j] += inf;
        committed += 1;
    }
    if committed > 0 {
        tracing::trace!(tick, events = committed, "interactions committed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer_energy_rule(probability: f32) -> CompiledRule {
        CompiledRule {
            radius: 8.0,
            self_cond: QuantityCondition::ANY,
            other_cond: QuantityCondition::ANY,
            probability,
            transfer_matter: 0.0,
            transfer_energy: 0.1,
            transfer_information: 0.0,
        }
    }

    fn two_particle_setup() -> (ParticleStore, GridGeom) {
        let geom = GridGeom::new(64.0, 64.0, 8.0);
        let mut s = ParticleStore::default();
        s.push(0, 30.0, 30.0, 0.0, 0.0, 1.0, 1.0, 0.0);
        s.push(1, 33.0, 30.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.canonicalize(&geom);
        (s, geom)
    }

    #[test]
    fn certain_rule_transfers() {
        let (mut s, geom) = two_particle_setup();
        // Gate on initiator energy so only the rich particle donates —
        // an ungated symmetric rule would transfer back and cancel out.
        let mut rule = transfer_energy_rule(1.0);
        rule.self_cond.energy = Bounds {
            min: 0.5,
            max: f32::INFINITY,
        };
        let rules = RuleSet { rules: vec![rule] };
        apply(&mut s, &geom, &rules, 7, 0);
        let total: f32 = s.energy.iter().sum();
        assert!((total - 1.0).abs() < 1e-5, "energy total changed: {total}");
        assert!(s.energy.iter().all(|&e| e >= 0.0));
        let donor = s.energy[s.id.iter().position(|&x| x == 0).unwrap()];
        let receiver = s.energy[s.id.iter().position(|&x| x == 1).unwrap()];
        assert!((donor - 0.9).abs() < 1e-5, "donor should lose 0.1: {donor}");
        assert!(
            (receiver - 0.1).abs() < 1e-5,
            "receiver should gain 0.1: {receiver}"
        );
    }

    #[test]
    fn zero_probability_never_fires() {
        let (mut s, geom) = two_particle_setup();
        let before = (s.energy.clone(), s.matter.clone());
        let rules = RuleSet {
            rules: vec![transfer_energy_rule(0.0)],
        };
        for tick in 0..100 {
            apply(&mut s, &geom, &rules, 7, tick);
        }
        assert_eq!(before.0, s.energy);
        assert_eq!(before.1, s.matter);
    }

    #[test]
    fn transfers_clamp_and_never_go_negative() {
        let geom = GridGeom::new(64.0, 64.0, 8.0);
        let mut s = ParticleStore::default();
        // Donor with almost nothing.
        s.push(0, 30.0, 30.0, 0.0, 0.0, 1e-5, 0.01, 0.0);
        s.push(1, 33.0, 30.0, 0.0, 0.0, 1.0, 1.0, 0.0);
        s.canonicalize(&geom);
        let rules = RuleSet {
            rules: vec![CompiledRule {
                radius: 8.0,
                self_cond: QuantityCondition::ANY,
                other_cond: QuantityCondition::ANY,
                probability: 1.0,
                transfer_matter: 0.5,
                transfer_energy: 0.5,
                transfer_information: 0.5,
            }],
        };
        for tick in 0..50 {
            apply(&mut s, &geom, &rules, 7, tick);
        }
        for i in 0..s.len() {
            assert!(
                s.matter[i] >= MIN_MATTER * 0.99,
                "matter floor broken: {}",
                s.matter[i]
            );
            assert!(s.energy[i] >= 0.0);
            assert!(s.information[i] >= 0.0);
        }
    }

    #[test]
    fn conditions_gate_events() {
        let (mut s, geom) = two_particle_setup();
        // Rule requires initiator energy >= 10 — nobody qualifies.
        let mut rule = transfer_energy_rule(1.0);
        rule.self_cond.energy = Bounds {
            min: 10.0,
            max: f32::INFINITY,
        };
        let rules = RuleSet { rules: vec![rule] };
        let before = s.energy.clone();
        apply(&mut s, &geom, &rules, 7, 0);
        assert_eq!(before, s.energy);
    }

    #[test]
    fn rule_fields_roundtrip() {
        let rule = CompiledRule {
            radius: 3.5,
            self_cond: QuantityCondition {
                matter: Bounds { min: 0.5, max: 2.0 },
                energy: Bounds::ANY,
                information: Bounds {
                    min: 1.0,
                    max: f32::INFINITY,
                },
            },
            other_cond: QuantityCondition::ANY,
            probability: 0.25,
            transfer_matter: 0.0,
            transfer_energy: 0.1,
            transfer_information: 0.05,
        };
        assert_eq!(CompiledRule::from_fields(rule.fields()), rule);
    }
}
