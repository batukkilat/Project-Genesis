//! Search step 1 (docs/research/search-design.md): schema-bounded mutation
//! operators over (SimConfig, RulePack) plus the ancestry record that makes
//! every mutant reproducible. The generation loop is step 2; nothing here
//! executes a simulation.
//!
//! Determinism: every mutation draws from a stream derived as
//! `DetRng::derive(search_seed, [generation, individual])` — the order-free
//! construction the impact payload uses — so a mutant is a pure function of
//! (search seed, generation, index, parent content). No draw counters, no
//! sequencing between individuals.
//!
//! Validity: operators repair-clamp into schema bounds and the result is
//! re-validated through the same `validate()` paths the loaders use. The one
//! cross-file invariant (rule radius ≤ physics interaction radius — the
//! assembly check panics on violation) is enforced by clamping rule radii;
//! v1 never mutates `interaction_radius` itself (nor world geometry, LOD,
//! env — evaluation cost and metric semantics must stay comparable, findings
//! 4 and 5 of the 2026-07-13 baseline sweep).

use genesis_config::{BoundsSpec, ConditionSpec, ConfigError, RulePack, SimConfig};
use genesis_core::DetRng;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Stream tag for mutation draws (ASCII "MUT"), keeping search streams
/// disjoint from any simulation stream family by construction.
const MUTATE_TAG: u64 = 0x4d5554;

/// One applied mutation, recorded exactly (field path, old, new) so ancestry
/// can be audited and replayed by eye.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MutationOp {
    /// Multiplicative jitter of one config scalar.
    JitterConfig { field: String, old: f32, new: f32 },
    /// Multiplicative jitter of one rule scalar.
    JitterRule {
        rule: usize,
        field: String,
        old: f32,
        new: f32,
    },
    /// Removed one rule (never the last).
    DropRule { rule: usize },
    /// Copied one rule and jittered fields of the copy — the schema-bounded
    /// analog of gene duplication. `fields` records (path, old, new) per
    /// jittered field of the new rule.
    DuplicateRule {
        source: usize,
        new_index: usize,
        fields: Vec<(String, f32, f32)>,
    },
    /// Moved one finite condition interval to a different quantity on the
    /// same rule side, leaving the source unbounded.
    RewireCondition {
        rule: usize,
        side: String,
        from: String,
        to: String,
    },
}

/// Ancestry sidecar for one individual (RON on disk): everything needed to
/// reproduce it — parent, the exact operators applied, and the derivation
/// coordinates of the RNG stream that chose them. Seeds from the corpus have
/// `parent: None, ops: []`. Lives above the engine, never replay identity
/// (the BranchRecord precedent, Q-2026-07-10-A).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "AncestryRecordDe")]
pub struct AncestryRecord {
    /// This individual's id, e.g. "g003-i07".
    pub id: String,
    /// Parent individual's id; None for corpus seeds.
    pub parent: Option<String>,
    /// The mutations that produced this individual from the parent, in
    /// application order — one entry per step, all drawn sequentially from
    /// the one derivation stream (so `genesis mutate --steps ops.len()`
    /// reproduces the chain). Sidecars written before multi-mutation
    /// children existed (search-01) carry a single `op` field instead;
    /// both forms load.
    pub ops: Vec<MutationOp>,
    /// Search master seed and this individual's derivation coordinates:
    /// stream = derive(search_seed, [MUTATE_TAG, generation, individual]).
    pub search_seed: u64,
    pub generation: u64,
    pub individual: u64,
    /// Paths of this individual's config and pack, relative to the search
    /// output directory.
    pub config: String,
    pub rules: String,
}

/// Deserialization shim: accepts both the current `ops` list and the
/// pre-multi-mutation single-`op` form the committed search-01 sidecars use.
/// When both are present (never written by any build), the list wins.
#[derive(Deserialize)]
struct AncestryRecordDe {
    id: String,
    parent: Option<String>,
    #[serde(default)]
    op: Option<MutationOp>,
    #[serde(default)]
    ops: Vec<MutationOp>,
    search_seed: u64,
    generation: u64,
    individual: u64,
    config: String,
    rules: String,
}

impl From<AncestryRecordDe> for AncestryRecord {
    fn from(de: AncestryRecordDe) -> Self {
        let ops = if de.ops.is_empty() {
            de.op.into_iter().collect()
        } else {
            de.ops
        };
        AncestryRecord {
            id: de.id,
            parent: de.parent,
            ops,
            search_seed: de.search_seed,
            generation: de.generation,
            individual: de.individual,
            config: de.config,
            rules: de.rules,
        }
    }
}

impl AncestryRecord {
    pub fn to_ron(&self) -> Result<String, ConfigError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| ConfigError::Parse(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        ron::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))
    }
}

/// The mutation stream for one individual: order-free derivation.
pub fn mutation_rng(search_seed: u64, generation: u64, individual: u64) -> DetRng {
    DetRng::derive(search_seed, &[MUTATE_TAG, generation, individual])
}

/// Search fitness v1 (docs/research/search-design.md, form B): a product of
/// saturating terms, so no single axis can dominate —
///
/// `ln(1 + structures_final) × ln(1 + lifetime_peak) ×
///  (1 + ln(1 + ln(1 + information_final)))`
///
/// - one immortal blob (`structures_final = 1`) is crushed by the first
///   term — the raw headline scalar's condensation exploit (baseline sweep
///   finding 1) does not pay here;
/// - a spray of momentary fragments is crushed by the second;
/// - structure-held information helps but saturates twice, because its
///   scale is config-dependent (info-rich worlds must not win on units).
///
/// The raw `persistence_complexity` scalar is deliberately not an input:
/// it stays *reported* in every record (the phase exit criterion is judged
/// on it) while selection climbs this function. Exact form to be ratified
/// in the decisions log when the generation loop lands; keep any change
/// here in lockstep with the design doc.
pub fn fitness(s: &genesis_observer::RunScore) -> f64 {
    (1.0 + s.structures_final as f64).ln()
        * (1.0 + s.lifetime_peak as f64).ln()
        * (1.0 + (1.0 + (1.0 + s.information_final.max(0.0)).ln()).ln())
}

/// `old * exp(u·sigma)` with u uniform in [-1, 1] — multiplicative, so
/// positive scales stay positive and proportionate. A zero value cannot
/// escape zero multiplicatively, so it restarts at a small fraction of
/// `scale` (the field's natural magnitude) instead.
fn jitter_value(rng: &mut DetRng, old: f32, sigma: f32, scale: f32) -> f32 {
    let u = rng.range_f32(-1.0, 1.0);
    if old == 0.0 {
        0.1 * scale * rng.next_f32()
    } else {
        old * (u * sigma).exp()
    }
}

/// Jitterable config scalars: (path, current, natural scale, clamp).
/// `interaction_radius` is deliberately absent (see module docs). Clamps
/// repair into the bounds `SimConfig::validate` enforces.
fn config_slots(config: &SimConfig) -> Vec<(&'static str, f32, f32)> {
    let p = &config.physics;
    let i = &config.initial;
    vec![
        ("physics.core_frac", p.core_frac, 0.4),
        ("physics.repulsion", p.repulsion, 40.0),
        ("physics.attraction", p.attraction, 5.0),
        ("physics.bond_rest_length", p.bond_rest_length, 3.0),
        ("physics.information_decay", p.information_decay, 0.02),
        ("initial.matter.lo", i.matter.lo, 0.5),
        ("initial.matter.hi", i.matter.hi, 1.0),
        ("initial.energy.lo", i.energy.lo, 0.5),
        ("initial.energy.hi", i.energy.hi, 1.0),
        ("initial.information.lo", i.information.lo, 0.5),
        ("initial.information.hi", i.information.hi, 1.0),
        ("initial.speed.lo", i.speed.lo, 1.0),
        ("initial.speed.hi", i.speed.hi, 2.0),
    ]
}

fn set_config_slot(config: &mut SimConfig, field: &str, v: f32) {
    let dt = config.dt();
    let p = &mut config.physics;
    let i = &mut config.initial;
    match field {
        // core_frac must be in (0, 1) — clamp well inside.
        "physics.core_frac" => p.core_frac = v.clamp(0.05, 0.95),
        "physics.repulsion" => p.repulsion = v.max(0.0),
        "physics.attraction" => p.attraction = v.max(0.0),
        "physics.bond_rest_length" => p.bond_rest_length = v.max(0.0),
        // decay * dt must stay <= 1.
        "physics.information_decay" => p.information_decay = v.clamp(0.0, 1.0 / dt),
        "initial.matter.lo" => i.matter.lo = v.max(1e-3),
        "initial.matter.hi" => i.matter.hi = v.max(1e-3),
        "initial.energy.lo" => i.energy.lo = v.max(0.0),
        "initial.energy.hi" => i.energy.hi = v.max(0.0),
        "initial.information.lo" => i.information.lo = v.max(0.0),
        "initial.information.hi" => i.information.hi = v.max(0.0),
        "initial.speed.lo" => i.speed.lo = v.max(0.0),
        "initial.speed.hi" => i.speed.hi = v.max(0.0),
        _ => unreachable!("unknown config slot {field}"),
    }
    // Range ends may cross after a jitter; repair by swap.
    for r in [
        &mut i.matter,
        &mut i.energy,
        &mut i.information,
        &mut i.speed,
    ] {
        if r.lo > r.hi {
            std::mem::swap(&mut r.lo, &mut r.hi);
        }
    }
}

/// Jitterable scalars of one rule: (path, current, natural scale). Condition
/// bounds participate only when finite (jittering ±∞ is meaningless).
fn rule_slots(pack: &RulePack, rule: usize) -> Vec<(String, f32, f32)> {
    let r = &pack.rules[rule];
    let mut slots = vec![
        ("radius".to_string(), r.radius, 5.0),
        ("probability".to_string(), r.probability, 0.05),
        ("transfer.matter".to_string(), r.transfer.matter, 0.01),
        ("transfer.energy".to_string(), r.transfer.energy, 0.03),
        (
            "transfer.information".to_string(),
            r.transfer.information,
            0.03,
        ),
    ];
    for (side, cond) in [("self_cond", &r.self_cond), ("other_cond", &r.other_cond)] {
        for (q, b) in [
            ("matter", cond.matter),
            ("energy", cond.energy),
            ("information", cond.information),
        ] {
            if b.min.is_finite() {
                slots.push((format!("{side}.{q}.min"), b.min, 0.5));
            }
            if b.max.is_finite() {
                slots.push((format!("{side}.{q}.max"), b.max, 0.5));
            }
        }
    }
    if let Some(bond) = r.bond_create {
        slots.push(("bond_create.strength".to_string(), bond.strength, 2.0));
    }
    if let Some(copy) = r.info_copy {
        slots.push(("info_copy.cost".to_string(), copy.cost, 0.02));
        slots.push(("info_copy.noise".to_string(), copy.noise, 0.1));
    }
    if let Some(emit) = r.emit {
        slots.push(("emit.matter_frac".to_string(), emit.matter_frac, 0.5));
        slots.push(("emit.energy_frac".to_string(), emit.energy_frac, 0.5));
        slots.push(("emit.info_frac".to_string(), emit.info_frac, 0.5));
        slots.push(("emit.offset".to_string(), emit.offset, 1.0));
    }
    slots
}

/// Write one rule scalar, repair-clamping into schema bounds. `max_radius`
/// is the physics interaction radius — the assembly invariant.
fn set_rule_slot(pack: &mut RulePack, rule: usize, field: &str, v: f32, max_radius: f32) {
    let r = &mut pack.rules[rule];
    let mut parts = field.splitn(3, '.');
    match (
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
    ) {
        ("radius", _, _) => r.radius = v.clamp(0.1, max_radius),
        ("probability", _, _) => r.probability = v.clamp(0.0, 1.0),
        ("transfer", "matter", _) => r.transfer.matter = v.max(0.0),
        ("transfer", "energy", _) => r.transfer.energy = v.max(0.0),
        ("transfer", "information", _) => r.transfer.information = v.max(0.0),
        (side @ ("self_cond" | "other_cond"), q, end) => {
            let cond = if side == "self_cond" {
                &mut r.self_cond
            } else {
                &mut r.other_cond
            };
            let b = match q {
                "matter" => &mut cond.matter,
                "energy" => &mut cond.energy,
                "information" => &mut cond.information,
                _ => unreachable!("unknown condition quantity {q}"),
            };
            match end {
                "min" => b.min = v.min(b.max),
                "max" => b.max = v.max(b.min),
                _ => unreachable!("unknown bound end {end}"),
            }
        }
        ("bond_create", "strength", _) => {
            if let Some(bond) = &mut r.bond_create {
                bond.strength = v.max(0.1);
            }
        }
        ("info_copy", "cost", _) => {
            if let Some(copy) = &mut r.info_copy {
                copy.cost = v.max(0.0);
            }
        }
        ("info_copy", "noise", _) => {
            if let Some(copy) = &mut r.info_copy {
                copy.noise = v.clamp(0.0, 1.0);
            }
        }
        ("emit", "matter_frac", _) => {
            if let Some(emit) = &mut r.emit {
                emit.matter_frac = v.clamp(0.0, 1.0);
            }
        }
        ("emit", "energy_frac", _) => {
            if let Some(emit) = &mut r.emit {
                emit.energy_frac = v.clamp(0.0, 1.0);
            }
        }
        ("emit", "info_frac", _) => {
            if let Some(emit) = &mut r.emit {
                emit.info_frac = v.clamp(0.0, 1.0);
            }
        }
        ("emit", "offset", _) => {
            if let Some(emit) = &mut r.emit {
                emit.offset = v.max(0.1);
            }
        }
        _ => unreachable!("unknown rule slot {field}"),
    }
}

fn pick<T>(rng: &mut DetRng, items: &[T]) -> usize {
    debug_assert!(!items.is_empty());
    (rng.next_u64() % items.len() as u64) as usize
}

/// Apply one mutation to `(config, pack)` in place and return the record of
/// what happened. The result always satisfies `SimConfig::validate`,
/// `RulePack::validate`, and the rule-radius assembly invariant.
///
/// Operator mix: rule-level operators dominate (the pack is where emergence
/// content lives); a pack with no rules only ever receives config jitter.
pub fn mutate(
    config: &mut SimConfig,
    pack: &mut RulePack,
    rng: &mut DetRng,
    sigma: f32,
) -> MutationOp {
    let max_radius = config.physics.interaction_radius;
    // Weighted op choice: 3 jitter-rule, 2 jitter-config, 1 drop,
    // 1 duplicate, 1 rewire.
    let roll = rng.next_u64() % 8;
    let op = if pack.rules.is_empty() {
        0
    } else {
        match roll {
            0 | 1 => 0, // config jitter
            2..=4 => 1, // rule jitter
            5 => 2,     // drop
            6 => 3,     // duplicate
            _ => 4,     // rewire
        }
    };

    let applied = match op {
        0 => {
            let slots = config_slots(config);
            let i = pick(rng, &slots);
            let (field, old, scale) = slots[i];
            let new = jitter_value(rng, old, sigma, scale);
            set_config_slot(config, field, new);
            // Read back the post-clamp value so the record is exact.
            let new = config_slots(config)[i].1;
            MutationOp::JitterConfig {
                field: field.to_string(),
                old,
                new,
            }
        }
        1 => {
            let rule = pick(rng, &pack.rules);
            let slots = rule_slots(pack, rule);
            let i = pick(rng, &slots);
            let (field, old, scale) = slots[i].clone();
            let new = jitter_value(rng, old, sigma, scale);
            set_rule_slot(pack, rule, &field, new, max_radius);
            let new = rule_slots(pack, rule)
                .into_iter()
                .find(|(f, _, _)| *f == field)
                .map(|(_, v, _)| v)
                .unwrap_or(new);
            MutationOp::JitterRule {
                rule,
                field,
                old,
                new,
            }
        }
        2 if pack.rules.len() >= 2 => {
            let rule = pick(rng, &pack.rules);
            pack.rules.remove(rule);
            MutationOp::DropRule { rule }
        }
        3 => {
            let source = pick(rng, &pack.rules);
            pack.rules.push(pack.rules[source].clone());
            let new_index = pack.rules.len() - 1;
            // Jitter up to 3 distinct fields of the copy.
            let mut fields = Vec::new();
            for _ in 0..3 {
                let slots = rule_slots(pack, new_index);
                let i = pick(rng, &slots);
                let (field, old, scale) = slots[i].clone();
                if fields
                    .iter()
                    .any(|(f, _, _): &(String, f32, f32)| *f == field)
                {
                    continue;
                }
                let new = jitter_value(rng, old, sigma, scale);
                set_rule_slot(pack, new_index, &field, new, max_radius);
                let new = rule_slots(pack, new_index)
                    .into_iter()
                    .find(|(f, _, _)| *f == field)
                    .map(|(_, v, _)| v)
                    .unwrap_or(new);
                fields.push((field, old, new));
            }
            MutationOp::DuplicateRule {
                source,
                new_index,
                fields,
            }
        }
        _ => {
            // Rewire: move a finite condition interval to another quantity on
            // the same side. Falls back to rule jitter when the chosen rule
            // has no finite interval to move.
            let rule = pick(rng, &pack.rules);
            let side_is_self = rng.next_u64().is_multiple_of(2);
            let side_name = if side_is_self {
                "self_cond"
            } else {
                "other_cond"
            };
            let quantities = ["matter", "energy", "information"];
            let cond = if side_is_self {
                &pack.rules[rule].self_cond
            } else {
                &pack.rules[rule].other_cond
            };
            let bounded: Vec<usize> = [cond.matter, cond.energy, cond.information]
                .iter()
                .enumerate()
                .filter(|(_, b)| b.min.is_finite() || b.max.is_finite())
                .map(|(i, _)| i)
                .collect();
            if bounded.is_empty() {
                // Nothing to rewire on this side: degrade to a rule jitter,
                // still fully recorded.
                let slots = rule_slots(pack, rule);
                let i = pick(rng, &slots);
                let (field, old, scale) = slots[i].clone();
                let new = jitter_value(rng, old, sigma, scale);
                set_rule_slot(pack, rule, &field, new, max_radius);
                let new = rule_slots(pack, rule)
                    .into_iter()
                    .find(|(f, _, _)| *f == field)
                    .map(|(_, v, _)| v)
                    .unwrap_or(new);
                MutationOp::JitterRule {
                    rule,
                    field,
                    old,
                    new,
                }
            } else {
                let from = bounded[pick(rng, &bounded)];
                let others: Vec<usize> = (0..3).filter(|&i| i != from).collect();
                let to = others[pick(rng, &others)];
                let cond = if side_is_self {
                    &mut pack.rules[rule].self_cond
                } else {
                    &mut pack.rules[rule].other_cond
                };
                // BoundsSpec is Copy: read, blank the source, write the target.
                let read = |c: &ConditionSpec, i: usize| match i {
                    0 => c.matter,
                    1 => c.energy,
                    _ => c.information,
                };
                let write = |c: &mut ConditionSpec, i: usize, b: BoundsSpec| match i {
                    0 => c.matter = b,
                    1 => c.energy = b,
                    _ => c.information = b,
                };
                let interval = read(cond, from);
                write(cond, from, BoundsSpec::default());
                write(cond, to, interval);
                MutationOp::RewireCondition {
                    rule,
                    side: side_name.to_string(),
                    from: quantities[from].to_string(),
                    to: quantities[to].to_string(),
                }
            }
        }
    };

    debug_assert!(config.validate().is_ok(), "mutated config must stay valid");
    debug_assert!(pack.validate().is_ok(), "mutated pack must stay valid");
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn corpus() -> Vec<(SimConfig, RulePack)> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut out = Vec::new();
        for (config, pack) in [
            (None, "packs/chains.ron"),
            (None, "packs/churn.ron"),
            (None, "packs/actual.ron"),
            (Some("configs/env-gradient.ron"), "packs/bands.ron"),
            (Some("configs/sieve.ron"), "packs/sieve.ron"),
        ] {
            let config = match config {
                Some(p) => SimConfig::load(&root.join(p)).unwrap(),
                None => SimConfig::default(),
            };
            out.push((config, RulePack::load(&root.join(pack)).unwrap()));
        }
        out
    }

    #[test]
    fn mutations_preserve_validity_across_the_corpus() {
        for (ci, (base_config, base_pack)) in corpus().into_iter().enumerate() {
            for individual in 0..200u64 {
                let mut config = base_config.clone();
                let mut pack = base_pack.clone();
                let mut rng = mutation_rng(42, ci as u64, individual);
                // Chains of 5 mutations stress accumulated drift.
                for _ in 0..5 {
                    mutate(&mut config, &mut pack, &mut rng, 0.3);
                }
                config.validate().unwrap_or_else(|e| {
                    panic!("corpus {ci} individual {individual}: config invalid: {e:?}")
                });
                pack.validate().unwrap_or_else(|e| {
                    panic!("corpus {ci} individual {individual}: pack invalid: {e:?}")
                });
                for (i, r) in pack.rules.iter().enumerate() {
                    assert!(
                        r.radius <= config.physics.interaction_radius,
                        "corpus {ci} individual {individual}: rule {i} radius {} exceeds \
                         interaction radius {} — assembly would panic",
                        r.radius,
                        config.physics.interaction_radius
                    );
                }
                assert!(!pack.rules.is_empty(), "drop must never empty the pack");
            }
        }
    }

    #[test]
    fn same_coordinates_same_mutant() {
        let (base_config, base_pack) = &corpus()[0];
        let run = || {
            let mut config = base_config.clone();
            let mut pack = base_pack.clone();
            let mut rng = mutation_rng(7, 3, 11);
            let ops: Vec<MutationOp> = (0..4)
                .map(|_| mutate(&mut config, &mut pack, &mut rng, 0.3))
                .collect();
            (ops, ron::ser::to_string(&pack).unwrap())
        };
        assert_eq!(
            run(),
            run(),
            "mutation must be a pure function of (seed, gen, idx)"
        );
    }

    #[test]
    fn different_individuals_diverge() {
        let (base_config, base_pack) = &corpus()[0];
        let mutant = |individual| {
            let mut config = base_config.clone();
            let mut pack = base_pack.clone();
            let mut rng = mutation_rng(7, 0, individual);

            mutate(&mut config, &mut pack, &mut rng, 0.3)
        };
        let distinct: std::collections::BTreeSet<String> =
            (0..16).map(|i| format!("{:?}", mutant(i))).collect();
        assert!(
            distinct.len() > 8,
            "16 individuals produced only {} distinct first ops",
            distinct.len()
        );
    }

    #[test]
    fn zero_valued_fields_can_escape_zero() {
        // information_decay defaults to 0; multiplicative jitter alone would
        // pin it there forever.
        let mut rng = DetRng::new(5);
        let escaped = (0..64).any(|_| jitter_value(&mut rng, 0.0, 0.3, 0.02) > 0.0);
        assert!(escaped, "zero must be escapable");
        // And the restart is bounded by the field's natural scale.
        let mut rng = DetRng::new(6);
        for _ in 0..64 {
            let v = jitter_value(&mut rng, 0.0, 0.3, 0.02);
            assert!((0.0..=0.002).contains(&v));
        }
    }

    #[test]
    fn fitness_prefers_diverse_persistent_regimes_over_condensation() {
        use genesis_observer::RunScore;
        let mut condensed = RunScore::zero();
        condensed.structures_final = 1;
        condensed.lifetime_peak = 200;
        condensed.persistence_complexity = 4000.0; // the exploit
        let mut fragmented = RunScore::zero();
        fragmented.structures_final = 2000;
        fragmented.lifetime_peak = 1;
        let mut diverse = RunScore::zero();
        diverse.structures_final = 800;
        diverse.lifetime_peak = 200;
        assert!(
            fitness(&diverse) > 5.0 * fitness(&condensed),
            "one immortal blob must not outscore a diverse persistent regime"
        );
        assert!(
            fitness(&diverse) > 5.0 * fitness(&fragmented),
            "momentary fragments must not outscore persistence"
        );
        // Information helps, but saturates: 100x the information must gain
        // far less than 2x fitness.
        let mut informed = diverse;
        informed.information_final = 1000.0;
        let mut very_informed = diverse;
        very_informed.information_final = 100_000.0;
        assert!(fitness(&informed) > fitness(&diverse));
        assert!(fitness(&very_informed) < 1.5 * fitness(&informed));
    }

    #[test]
    fn fitness_reranks_the_committed_baseline_corpus() {
        // The 2026-07-13 baseline records are the evidence this function was
        // designed on: the raw headline scalar ranks the condensed worlds
        // (actual, bands) on top; fitness must instead put the
        // many-structures regimes (chains, full-stack) above them.
        use genesis_observer::ScoreRecord;
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/research/sweeps/2026-07-13-shipped-packs");
        let load = |name: &str| {
            ScoreRecord::load(&dir.join(format!("{name}.score.ron")))
                .unwrap_or_else(|e| panic!("{name}: {e:?}"))
        };
        let chains = load("chains");
        let full_stack = load("full-stack");
        let actual = load("actual");
        let bands = load("bands");
        // Raw scalar ranks condensation on top...
        assert!(
            actual.score.persistence_complexity > chains.score.persistence_complexity
                && bands.score.persistence_complexity > chains.score.persistence_complexity
        );
        // ...fitness inverts that.
        for condensed in [&actual, &bands] {
            for diverse in [&chains, &full_stack] {
                assert!(
                    fitness(&diverse.score) > fitness(&condensed.score),
                    "{:?} must outrank {:?}",
                    diverse.rules,
                    condensed.rules
                );
            }
        }
        // Bond-free regimes score zero fitness, mirroring their zero scores.
        assert_eq!(fitness(&load("diffusion").score), 0.0);
    }

    #[test]
    fn ancestry_record_roundtrips() {
        let rec = AncestryRecord {
            id: "g003-i07".into(),
            parent: Some("g002-i01".into()),
            ops: vec![
                MutationOp::JitterRule {
                    rule: 2,
                    field: "probability".into(),
                    old: 0.1,
                    new: 0.117,
                },
                MutationOp::DropRule { rule: 0 },
            ],
            search_seed: 42,
            generation: 3,
            individual: 7,
            config: "g003/i07.config.ron".into(),
            rules: "g003/i07.pack.ron".into(),
        };
        let dir =
            std::env::temp_dir().join(format!("genesis-ancestry-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rec.ron");
        std::fs::write(&path, rec.to_ron().unwrap()).unwrap();
        let back = AncestryRecord::load(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back, rec);
    }

    /// Sidecars written before multi-mutation children existed carry a
    /// single `op: Option<MutationOp>` — the committed search-01 artifacts
    /// are in this form and must keep loading.
    #[test]
    fn ancestry_record_loads_pre_multi_mutation_form() {
        let legacy = r#"(
    id: "g005-i004",
    parent: Some("g004-i007"),
    op: Some(RewireCondition(
        rule: 2,
        side: "self_cond",
        from: "energy",
        to: "matter",
    )),
    search_seed: 20260713,
    generation: 5,
    individual: 4,
    config: "g005/g005-i004.config.ron",
    rules: "g005/g005-i004.pack.ron",
)"#;
        let rec: AncestryRecord = ron::from_str(legacy).unwrap();
        assert_eq!(
            rec.ops,
            vec![MutationOp::RewireCondition {
                rule: 2,
                side: "self_cond".into(),
                from: "energy".into(),
                to: "matter".into(),
            }]
        );
        // A seed sidecar in the legacy form: op: None → empty ops.
        let seed_legacy = r#"(
    id: "g000-i001",
    parent: None,
    op: None,
    search_seed: 20260713,
    generation: 0,
    individual: 1,
    config: "g000/g000-i001.config.ron",
    rules: "g000/g000-i001.pack.ron",
)"#;
        let rec: AncestryRecord = ron::from_str(seed_legacy).unwrap();
        assert!(rec.ops.is_empty());
        assert!(rec.parent.is_none());
    }
}
