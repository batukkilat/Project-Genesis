//! Layer 1: the Interaction System — discrete, rule-driven state transitions.
//!
//! Phase 3: the full action vocabulary — quantity transfers, bond
//! create/break, lossy information copy (with energy cost), and particle
//! emit/absorb — following the constitution pipeline (condition →
//! probability → cost → action) with a deterministic two-phase
//! collect-then-commit step.
//!
//! Rules here are the *compiled internal representation*; the authoring
//! format is the RON schema in `genesis_config::rules` (decisions log Q2),
//! which compiles down to these structs.
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

/// Bond effect of a rule firing on the pair. `Create` is a no-op if the pair
/// is already bonded (bonds never stack); `Break` is a no-op if it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BondAction {
    #[default]
    None,
    Create,
    Break,
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
    pub bond_action: BondAction,
    /// Spring stiffness of a created bond; meaningful only for
    /// `BondAction::Create`.
    pub bond_strength: f32,
    /// Lossy information copy: initiator's information overwrites the
    /// other's, degraded by `info_noise`. The initiator pays `info_cost`
    /// energy to the other particle (energy conserved); an unpayable cost
    /// aborts the whole event, transfers included.
    pub info_copy: bool,
    pub info_cost: f32,
    /// Noise fraction in [0, 1]: copied value is `src * (1 + noise * u)`,
    /// `u` uniform in [-1, 1], clamped >= 0.
    pub info_noise: f32,
    /// Emit (split): spawn one particle from the initiator's stocks, moving
    /// the given fractions — conserved by construction. Child inherits the
    /// initiator's velocity (momentum-exact) and appears `emit_offset` away
    /// at a per-pair deterministic angle. Aborts if the matter split would
    /// leave either side below `MIN_MATTER`.
    pub emit: bool,
    pub emit_matter_frac: f32,
    pub emit_energy_frac: f32,
    pub emit_info_frac: f32,
    pub emit_offset: f32,
    /// Absorb (merge): all of the other particle's quantities move to the
    /// initiator, velocity becomes the mass-weighted average, and the other
    /// is destroyed. Its bonds are pruned at the next rebuild.
    pub absorb: bool,
}

/// The active rule set. Part of replay identity — its content is hashed into
/// the state hash and serialized into saves.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct RuleSet {
    pub rules: Vec<CompiledRule>,
}

impl RuleSet {
    /// Compile an authored rule pack into the internal representation. The
    /// pack must already pass `RulePack::validate`; grid-dependent checks
    /// (`assert_valid`) run at simulation assembly.
    pub fn compile(pack: &genesis_config::RulePack) -> RuleSet {
        let bounds = |b: genesis_config::BoundsSpec| Bounds {
            min: b.min,
            max: b.max,
        };
        let cond = |c: genesis_config::ConditionSpec| QuantityCondition {
            matter: bounds(c.matter),
            energy: bounds(c.energy),
            information: bounds(c.information),
        };
        RuleSet {
            rules: pack
                .rules
                .iter()
                .map(|r| CompiledRule {
                    radius: r.radius,
                    self_cond: cond(r.self_cond),
                    other_cond: cond(r.other_cond),
                    probability: r.probability,
                    transfer_matter: r.transfer.matter,
                    transfer_energy: r.transfer.energy,
                    transfer_information: r.transfer.information,
                    bond_action: match (r.bond_create, r.bond_break) {
                        (Some(_), _) => BondAction::Create,
                        (None, true) => BondAction::Break,
                        (None, false) => BondAction::None,
                    },
                    bond_strength: r.bond_create.map_or(0.0, |b| b.strength),
                    info_copy: r.info_copy.is_some(),
                    info_cost: r.info_copy.map_or(0.0, |c| c.cost),
                    info_noise: r.info_copy.map_or(0.0, |c| c.noise),
                    emit: r.emit.is_some(),
                    emit_matter_frac: r.emit.map_or(0.0, |e| e.matter_frac),
                    emit_energy_frac: r.emit.map_or(0.0, |e| e.energy_frac),
                    emit_info_frac: r.emit.map_or(0.0, |e| e.info_frac),
                    emit_offset: r.emit.map_or(0.0, |e| e.offset),
                    absorb: r.absorb,
                })
                .collect(),
        }
    }

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
            if r.bond_action == BondAction::Create {
                assert!(
                    r.bond_strength > 0.0 && r.bond_strength.is_finite(),
                    "rule {i}: bond strength {} must be positive and finite",
                    r.bond_strength
                );
            }
            if r.info_copy {
                assert!(
                    r.info_cost >= 0.0 && r.info_cost.is_finite(),
                    "rule {i}: info_cost {} must be >= 0 and finite",
                    r.info_cost
                );
                assert!(
                    (0.0..=1.0).contains(&r.info_noise),
                    "rule {i}: info_noise {} outside [0, 1]",
                    r.info_noise
                );
            }
            if r.emit {
                assert!(
                    !r.absorb,
                    "rule {i}: emit and absorb are mutually exclusive"
                );
                for (name, frac) in [
                    ("matter", r.emit_matter_frac),
                    ("energy", r.emit_energy_frac),
                    ("info", r.emit_info_frac),
                ] {
                    assert!(
                        (0.0..=1.0).contains(&frac),
                        "rule {i}: emit {name} fraction {frac} outside [0, 1]"
                    );
                }
                assert!(
                    r.emit_offset > 0.0 && r.emit_offset.is_finite(),
                    "rule {i}: emit offset {} must be positive and finite \
                     (a child at distance zero would never separate)",
                    r.emit_offset
                );
            }
        }
    }
}

impl CompiledRule {
    /// Canonical field order, used by hashing and serialization. Keep in
    /// sync with `from_fields`. The bond action is encoded as a code float
    /// (0 = none, 1 = create, 2 = break); `info_copy`, `emit`, and `absorb`
    /// as 0/1.
    pub fn fields(&self) -> [f32; 28] {
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
            match self.bond_action {
                BondAction::None => 0.0,
                BondAction::Create => 1.0,
                BondAction::Break => 2.0,
            },
            self.bond_strength,
            if self.info_copy { 1.0 } else { 0.0 },
            self.info_cost,
            self.info_noise,
            if self.emit { 1.0 } else { 0.0 },
            self.emit_matter_frac,
            self.emit_energy_frac,
            self.emit_info_frac,
            self.emit_offset,
            if self.absorb { 1.0 } else { 0.0 },
        ]
    }

    /// Inverse of `fields`. An unknown bond-action code decodes to `None`
    /// rather than panicking — a corrupt save still fails cleanly at its
    /// integrity-hash check instead of aborting mid-parse.
    pub fn from_fields(f: [f32; 28]) -> Self {
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
            bond_action: if f[17] == 1.0 {
                BondAction::Create
            } else if f[17] == 2.0 {
                BondAction::Break
            } else {
                BondAction::None
            },
            bond_strength: f[18],
            info_copy: f[19] == 1.0,
            info_cost: f[20],
            info_noise: f[21],
            emit: f[22] == 1.0,
            emit_matter_frac: f[23],
            emit_energy_frac: f[24],
            emit_info_frac: f[25],
            emit_offset: f[26],
            absorb: f[27] == 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Intent {
    initiator: u32,
    other: u32,
    rule: u32,
    /// Uniform [0, 1) draw for info-copy noise; second draw from the same
    /// per-pair derived stream as the probability roll, so it is fixed by
    /// (tick, ids, rule) alone. 0 when the rule copies nothing.
    noise_u: f32,
    /// Uniform [0, 1) draw for the emit angle; next draw from the same
    /// stream. 0 when the rule emits nothing. Draw order per rule is fixed
    /// by the rule's own flags, so it is identical in every run.
    emit_u: f32,
}

/// Run one interaction step: collect intents in parallel, commit sequentially
/// in canonical order. Store must be canonicalized and positions must not
/// have moved since (call between `forces` and `integrate`). Bond edits land
/// on the edge list only; the CSR mirror goes stale but nothing reads it
/// again until the next tick's rebuild.
///
/// Structural changes are safe mid-commit by construction: emissions append
/// to the store (existing intent indices stay valid; the appended particles
/// are never referenced this tick), absorptions mark the victim dead and the
/// store compacts once after all intents. Any intent touching a dead
/// particle is skipped whole. Child ids come from `next_id` in commit order,
/// so id assignment is as deterministic as everything else.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    store: &mut ParticleStore,
    bonds: &mut crate::bonds::BondStore,
    geom: &GridGeom,
    rules: &RuleSet,
    stream_seed: u64,
    tick: u64,
    next_id: &mut u64,
    information_max: f32,
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
    // Adaptive-detail gate: an interaction event fires only between two active
    // particles. Matter/energy conservation is untouched — each committed
    // event still conserves per the constitution; a frozen particle simply
    // never participates, so there is no partial or one-sided transfer to
    // leak. An empty mask means "not tracked" (LOD off) → every particle
    // active, so this is exactly the pre-LOD behavior. `lod` also drives the
    // emit path below, keeping `store.active` aligned with the store.
    let active: &[bool] = &store.active;
    let lod = active.len() == n;

    let n_chunks = n.div_ceil(par_chunk());
    let intent_chunks: Vec<Vec<Intent>> = (0..n_chunks)
        .into_par_iter()
        .map(|c| {
            let lo = c * par_chunk();
            let hi = (lo + par_chunk()).min(n);
            let mut out = Vec::new();
            for i in lo..hi {
                if lod && !active[i] {
                    continue;
                }
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
                            if lod && !active[j] {
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
                            let mut pair_rng =
                                DetRng::derive(stream_seed, &[tick, id[i], id[j], ridx as u64]);
                            if pair_rng.next_f32() < rule.probability {
                                let noise_u = if rule.info_copy {
                                    pair_rng.next_f32()
                                } else {
                                    0.0
                                };
                                let emit_u = if rule.emit { pair_rng.next_f32() } else { 0.0 };
                                out.push(Intent {
                                    initiator: i as u32,
                                    other: j as u32,
                                    rule: ridx as u32,
                                    noise_u,
                                    emit_u,
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
    // Liveness only matters when some rule can destroy; empty = all alive.
    let mut alive: Vec<bool> = if rules.rules.iter().any(|r| r.absorb) {
        vec![true; n]
    } else {
        Vec::new()
    };

    let mut committed = 0u64;
    for intent in intent_chunks.iter().flatten() {
        let rule = &rules.rules[intent.rule as usize];
        let i = intent.initiator as usize;
        let j = intent.other as usize;

        // A destroyed endpoint invalidates the whole intent.
        if !alive.is_empty() && (!alive[i] || !alive[j]) {
            continue;
        }

        // Costs gate the whole event and apply before any action
        // (constitution pipeline: check payable — abort if not — then pay,
        // then act). Paying first also stops the event's own energy
        // transfer from draining the stock the cost was checked against.
        // The payment moves to the receiver so energy stays conserved.
        // The emit feasibility check is a gate too: nothing about the event
        // may happen if the split would break the mass floor. Between this
        // gate and the emit below, only energy moves — matter is untouched,
        // so the check stays valid.
        if rule.info_copy && store.energy[i] < rule.info_cost {
            continue;
        }
        if rule.emit {
            let cm = rule.emit_matter_frac * store.matter[i];
            if !(cm >= MIN_MATTER && store.matter[i] - cm >= MIN_MATTER) {
                continue;
            }
        }
        if rule.info_copy {
            store.energy[i] -= rule.info_cost;
            store.energy[j] += rule.info_cost;
        }

        if rule.emit {
            let cm = rule.emit_matter_frac * store.matter[i];
            let ce = rule.emit_energy_frac * store.energy[i];
            // Cap the child's information at birth; the parent subtracts the
            // same (clamped) amount, so parent and child stay balanced.
            let ci = (rule.emit_info_frac * store.information[i]).min(information_max);
            store.matter[i] -= cm;
            store.energy[i] -= ce;
            store.information[i] -= ci;
            let angle = std::f32::consts::TAU * intent.emit_u;
            let cx =
                genesis_core::torus::wrap(store.px[i] + rule.emit_offset * angle.cos(), world.0);
            let cy =
                genesis_core::torus::wrap(store.py[i] + rule.emit_offset * angle.sin(), world.1);
            let child_id = *next_id;
            *next_id += 1;
            store.push(child_id, cx, cy, store.vx[i], store.vy[i], cm, ce, ci);
            // Keep the force accumulators index-aligned for `integrate`;
            // the child feels no force on its birth tick.
            store.fx.push(0.0);
            store.fy.push(0.0);
            // Keep the activity mask aligned too (when tracked): a child is
            // active on its birth tick — its parent was active to emit it — so
            // it integrates this tick like any active particle.
            if lod {
                store.active.push(true);
            }
        }

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

        match rule.bond_action {
            BondAction::None => {}
            BondAction::Create => {
                bonds.create(store.id[i], store.id[j], rule.bond_strength);
            }
            BondAction::Break => {
                bonds.remove(store.id[i], store.id[j]);
            }
        }

        if rule.info_copy {
            // Cost already paid above. The copy is lossy and creates
            // information — deliberately not conserved (decisions log).
            let u = 2.0 * intent.noise_u - 1.0; // uniform [-1, 1)
            store.information[j] = (store.information[i] * (1.0 + rule.info_noise * u)).max(0.0);
        }

        if rule.absorb {
            // Perfectly inelastic merge: all stocks move, velocity becomes
            // the mass-weighted average (momentum-exact to f32 rounding).
            // The victim's bonds dangle and prune at the next rebuild.
            let mi = store.matter[i];
            let mj = store.matter[j];
            let msum = mi + mj;
            store.vx[i] = (mi * store.vx[i] + mj * store.vx[j]) / msum;
            store.vy[i] = (mi * store.vy[i] + mj * store.vy[j]) / msum;
            store.matter[i] = msum;
            store.energy[i] += store.energy[j];
            store.information[i] += store.information[j];
            alive[j] = false;
        }

        // Information overflow cap (decisions log, Q-2026-07-06-B): every
        // information write this event touched i and/or j; clamp both so
        // amplifying rules saturate at information_max instead of running to
        // f32 overflow / NaN. Lower bound 0 is already held by each write's
        // `.max(0.0)`. Matter and energy are conserved by construction and
        // stay uncapped. A clamp on the (possibly dead) victim j is harmless.
        store.information[i] = store.information[i].min(information_max);
        store.information[j] = store.information[j].min(information_max);
        committed += 1;
    }

    if alive.contains(&false) {
        store.remove_dead(&alive);
    }
    if committed > 0 {
        tracing::trace!(tick, events = committed, "interactions committed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bonds::BondStore;

    fn bonds_scratch() -> BondStore {
        BondStore::default()
    }

    fn transfer_energy_rule(probability: f32) -> CompiledRule {
        CompiledRule {
            radius: 8.0,
            self_cond: QuantityCondition::ANY,
            other_cond: QuantityCondition::ANY,
            probability,
            transfer_matter: 0.0,
            transfer_energy: 0.1,
            transfer_information: 0.0,
            bond_action: BondAction::None,
            bond_strength: 0.0,
            info_copy: false,
            info_cost: 0.0,
            info_noise: 0.0,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
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
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut 1_000_000u64,
            f32::INFINITY,
        );
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
            apply(
                &mut s,
                &mut bonds_scratch(),
                &geom,
                &rules,
                7,
                tick,
                &mut 1_000_000u64,
                f32::INFINITY,
            );
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
                bond_action: BondAction::None,
                bond_strength: 0.0,
                info_copy: false,
                info_cost: 0.0,
                info_noise: 0.0,
                emit: false,
                emit_matter_frac: 0.0,
                emit_energy_frac: 0.0,
                emit_info_frac: 0.0,
                emit_offset: 0.0,
                absorb: false,
            }],
        };
        for tick in 0..50 {
            apply(
                &mut s,
                &mut bonds_scratch(),
                &geom,
                &rules,
                7,
                tick,
                &mut 1_000_000u64,
                f32::INFINITY,
            );
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
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut 1_000_000u64,
            f32::INFINITY,
        );
        assert_eq!(before, s.energy);
    }

    #[test]
    fn compile_maps_pack_faithfully() {
        let pack = genesis_config::RulePack::example();
        let set = RuleSet::compile(&pack);
        assert_eq!(set.rules.len(), pack.rules.len());
        let r = &set.rules[0];
        let spec = &pack.rules[0];
        assert_eq!(r.radius, spec.radius);
        assert_eq!(r.probability, spec.probability);
        assert_eq!(r.self_cond.energy.min, spec.self_cond.energy.min);
        assert_eq!(r.transfer_energy, spec.transfer.energy);
        // Omitted bounds compile to ANY.
        assert_eq!(r.self_cond.matter.min, f32::NEG_INFINITY);
        assert_eq!(r.self_cond.matter.max, f32::INFINITY);
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
            bond_action: BondAction::Create,
            bond_strength: 1.5,
            info_copy: false,
            info_cost: 0.0,
            info_noise: 0.0,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
        };
        assert_eq!(CompiledRule::from_fields(rule.fields()), rule);
        let broken = CompiledRule {
            bond_action: BondAction::Break,
            bond_strength: 0.0,
            info_copy: false,
            info_cost: 0.0,
            info_noise: 0.0,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
            ..rule
        };
        assert_eq!(CompiledRule::from_fields(broken.fields()), broken);
        let copying = CompiledRule {
            info_copy: true,
            info_cost: 0.3,
            info_noise: 0.1,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
            ..rule
        };
        assert_eq!(CompiledRule::from_fields(copying.fields()), copying);
    }

    fn bond_rule(action: BondAction) -> CompiledRule {
        CompiledRule {
            radius: 8.0,
            self_cond: QuantityCondition::ANY,
            other_cond: QuantityCondition::ANY,
            probability: 1.0,
            transfer_matter: 0.0,
            transfer_energy: 0.0,
            transfer_information: 0.0,
            bond_action: action,
            bond_strength: if action == BondAction::Create {
                2.0
            } else {
                0.0
            },
            info_copy: false,
            info_cost: 0.0,
            info_noise: 0.0,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
        }
    }

    #[test]
    fn bond_create_rule_makes_one_edge() {
        // Both orderings of the pair fire, but bonds never stack: exactly
        // one edge results, and repeating the tick stays at one.
        let (mut s, geom) = two_particle_setup();
        let mut bonds = BondStore::default();
        let rules = RuleSet {
            rules: vec![bond_rule(BondAction::Create)],
        };
        apply(
            &mut s,
            &mut bonds,
            &geom,
            &rules,
            7,
            0,
            &mut 1_000_000u64,
            f32::INFINITY,
        );
        assert_eq!(bonds.len(), 1);
        assert!(bonds.contains(0, 1));
        assert_eq!(bonds.strength[0], 2.0);
        apply(
            &mut s,
            &mut bonds,
            &geom,
            &rules,
            7,
            1,
            &mut 1_000_000u64,
            f32::INFINITY,
        );
        assert_eq!(bonds.len(), 1);
    }

    #[test]
    fn bond_break_rule_removes_the_edge() {
        let (mut s, geom) = two_particle_setup();
        let mut bonds = BondStore::default();
        bonds.create(0, 1, 2.0);
        let rules = RuleSet {
            rules: vec![bond_rule(BondAction::Break)],
        };
        apply(
            &mut s,
            &mut bonds,
            &geom,
            &rules,
            7,
            0,
            &mut 1_000_000u64,
            f32::INFINITY,
        );
        assert!(bonds.is_empty());
    }

    /// Copy rule gated so only the info-rich particle initiates.
    fn copy_rule(cost: f32, noise: f32) -> CompiledRule {
        CompiledRule {
            radius: 8.0,
            self_cond: QuantityCondition {
                matter: Bounds::ANY,
                energy: Bounds::ANY,
                information: Bounds {
                    min: 0.5,
                    max: f32::INFINITY,
                },
            },
            other_cond: QuantityCondition::ANY,
            probability: 1.0,
            transfer_matter: 0.0,
            transfer_energy: 0.0,
            transfer_information: 0.0,
            bond_action: BondAction::None,
            bond_strength: 0.0,
            info_copy: true,
            info_cost: cost,
            info_noise: noise,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
        }
    }

    fn copy_setup() -> (ParticleStore, GridGeom) {
        let geom = GridGeom::new(64.0, 64.0, 8.0);
        let mut s = ParticleStore::default();
        // id 0: info-rich initiator; id 1: blank receiver.
        s.push(0, 30.0, 30.0, 0.0, 0.0, 1.0, 1.0, 0.8);
        s.push(1, 33.0, 30.0, 0.0, 0.0, 1.0, 1.0, 0.1);
        s.canonicalize(&geom);
        (s, geom)
    }

    #[test]
    fn noiseless_copy_imprints_exactly() {
        let (mut s, geom) = copy_setup();
        let rules = RuleSet {
            rules: vec![copy_rule(0.0, 0.0)],
        };
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut 1_000_000u64,
            f32::INFINITY,
        );
        let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        assert_eq!(s.information[at(0)], 0.8, "source unchanged");
        assert_eq!(s.information[at(1)], 0.8, "receiver overwritten with copy");
        // Information was created, not moved: total went up.
        let total: f32 = s.information.iter().sum();
        assert!(total > 0.9);
    }

    #[test]
    fn unpayable_cost_aborts_whole_event() {
        let (mut s, geom) = copy_setup();
        // Cost above the initiator's 1.0 energy; rule also carries an energy
        // transfer, which must abort with the event.
        let mut rule = copy_rule(2.0, 0.0);
        rule.transfer_energy = 0.5;
        let rules = RuleSet { rules: vec![rule] };
        let (energy0, info0) = (s.energy.clone(), s.information.clone());
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut 1_000_000u64,
            f32::INFINITY,
        );
        assert_eq!(s.energy, energy0, "no partial cost, no transfer");
        assert_eq!(s.information, info0, "no copy");
    }

    #[test]
    fn copy_cost_moves_to_receiver() {
        let (mut s, geom) = copy_setup();
        let rules = RuleSet {
            rules: vec![copy_rule(0.25, 0.0)],
        };
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut 1_000_000u64,
            f32::INFINITY,
        );
        let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        assert!((s.energy[at(0)] - 0.75).abs() < 1e-6);
        assert!((s.energy[at(1)] - 1.25).abs() < 1e-6);
        let total: f32 = s.energy.iter().sum();
        assert!((total - 2.0).abs() < 1e-6, "energy conserved: {total}");
        assert_eq!(s.information[at(1)], 0.8);
    }

    #[test]
    fn information_saturates_at_max() {
        // A noiseless copy would set the receiver to the source's 0.8. With a
        // cap of 0.5 both the copy target and the source (touched by the
        // event) clamp to 0.5 instead — no overflow, no NaN.
        let (mut s, geom) = copy_setup();
        let rules = RuleSet {
            rules: vec![copy_rule(0.0, 0.0)],
        };
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut 1_000_000u64,
            0.5,
        );
        let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        assert_eq!(s.information[at(0)], 0.5, "source clamped to the cap");
        assert_eq!(s.information[at(1)], 0.5, "copy target clamped to the cap");
    }

    #[test]
    fn amplifying_copy_cannot_exceed_max_over_many_ticks() {
        // An amplifying copy (positive noise) run repeatedly would blow up
        // toward f32 overflow uncapped; the cap holds every particle at or
        // below information_max forever, and the result stays finite.
        let (mut s, geom) = copy_setup();
        let rules = RuleSet {
            rules: vec![copy_rule(0.0, 0.5)],
        };
        let cap = 2.0f32;
        for tick in 0..200 {
            apply(
                &mut s,
                &mut bonds_scratch(),
                &geom,
                &rules,
                7,
                tick,
                &mut 1_000_000u64,
                cap,
            );
        }
        for &v in &s.information {
            assert!(v.is_finite(), "information stayed finite under the cap");
            assert!(v <= cap, "information {v} exceeded the cap {cap}");
        }
    }

    fn base_rule() -> CompiledRule {
        CompiledRule {
            radius: 8.0,
            self_cond: QuantityCondition::ANY,
            other_cond: QuantityCondition::ANY,
            probability: 1.0,
            transfer_matter: 0.0,
            transfer_energy: 0.0,
            transfer_information: 0.0,
            bond_action: BondAction::None,
            bond_strength: 0.0,
            info_copy: false,
            info_cost: 0.0,
            info_noise: 0.0,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
        }
    }

    #[test]
    fn emit_splits_stocks_and_conserves() {
        let geom = GridGeom::new(64.0, 64.0, 8.0);
        let mut s = ParticleStore::default();
        s.push(0, 30.0, 30.0, 0.5, -0.25, 1.0, 1.0, 0.8);
        s.push(1, 33.0, 30.0, 0.0, 0.0, 0.2, 0.0, 0.0);
        s.canonicalize(&geom);
        // Gate on matter so only the heavy particle emits, one direction.
        let mut rule = base_rule();
        rule.self_cond.matter = Bounds {
            min: 0.9,
            max: f32::INFINITY,
        };
        rule.emit = true;
        rule.emit_matter_frac = 0.5;
        rule.emit_energy_frac = 0.25;
        rule.emit_info_frac = 1.0;
        rule.emit_offset = 1.5;
        let rules = RuleSet { rules: vec![rule] };
        let mut next_id = 100u64;
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut next_id,
            f32::INFINITY,
        );

        assert_eq!(s.len(), 3);
        assert_eq!(next_id, 101);
        let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        let (p, c) = (at(0), at(100));
        assert_eq!(s.matter[p], 0.5);
        assert_eq!(s.matter[c], 0.5);
        assert_eq!(s.energy[c], 0.25);
        assert_eq!(s.information[p], 0.0, "info_frac 1.0 moves everything");
        assert_eq!(s.information[c], 0.8);
        // Momentum-exact: child inherits parent velocity bit-for-bit.
        assert_eq!((s.vx[c], s.vy[c]), (0.5, -0.25));
        // Child appears at the configured offset.
        let dx = genesis_core::torus::delta(s.px[p], s.px[c], 64.0);
        let dy = genesis_core::torus::delta(s.py[p], s.py[c], 64.0);
        assert!(((dx * dx + dy * dy).sqrt() - 1.5).abs() < 1e-4);
        // Totals conserved.
        let mt: f32 = s.matter.iter().sum();
        let et: f32 = s.energy.iter().sum();
        assert!((mt - 1.2).abs() < 1e-6);
        assert!((et - 1.0).abs() < 1e-6);
    }

    #[test]
    fn emit_gate_aborts_whole_event_at_mass_floor() {
        let geom = GridGeom::new(64.0, 64.0, 8.0);
        let mut s = ParticleStore::default();
        s.push(0, 30.0, 30.0, 0.0, 0.0, 1.5e-6, 1.0, 0.0);
        s.push(1, 33.0, 30.0, 0.0, 0.0, 1.5e-6, 0.0, 0.0);
        s.canonicalize(&geom);
        let mut rule = base_rule();
        rule.emit = true;
        rule.emit_matter_frac = 0.5;
        rule.emit_offset = 1.0;
        rule.transfer_energy = 0.5; // must abort together with the emit
        let rules = RuleSet { rules: vec![rule] };
        let energy0 = s.energy.clone();
        let mut next_id = 100u64;
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut next_id,
            f32::INFINITY,
        );
        assert_eq!(s.len(), 2, "no child below the mass floor");
        assert_eq!(next_id, 100, "no id consumed by an aborted event");
        assert_eq!(s.energy, energy0, "transfers abort with the event");
    }

    #[test]
    fn absorb_merges_stocks_momentum_and_removes() {
        let geom = GridGeom::new(64.0, 64.0, 8.0);
        let mut s = ParticleStore::default();
        s.push(0, 30.0, 30.0, 1.0, 0.0, 3.0, 0.6, 0.1);
        s.push(1, 33.0, 30.0, -2.0, 0.0, 1.0, 0.4, 0.2);
        s.canonicalize(&geom);
        // Heavy absorbs light, one direction only.
        let mut rule = base_rule();
        rule.self_cond.matter = Bounds {
            min: 2.0,
            max: f32::INFINITY,
        };
        rule.other_cond.matter = Bounds {
            min: f32::NEG_INFINITY,
            max: 2.0,
        };
        rule.absorb = true;
        let rules = RuleSet { rules: vec![rule] };
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut 100u64,
            f32::INFINITY,
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s.id[0], 0);
        assert_eq!(s.matter[0], 4.0);
        assert!((s.energy[0] - 1.0).abs() < 1e-6);
        assert!((s.information[0] - 0.3).abs() < 1e-6);
        // Inelastic merge: (3*1 + 1*(-2)) / 4 = 0.25.
        assert!((s.vx[0] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn dead_particle_invalidates_later_intents() {
        let geom = GridGeom::new(64.0, 64.0, 8.0);
        let mut s = ParticleStore::default();
        // Two heavy initiators flank one light victim; both intents target
        // it, only the first may eat.
        s.push(0, 28.0, 30.0, 0.0, 0.0, 3.0, 0.0, 0.0);
        s.push(1, 30.0, 30.0, 0.0, 0.0, 1.0, 0.5, 0.0);
        s.push(2, 32.0, 30.0, 0.0, 0.0, 3.0, 0.0, 0.0);
        s.canonicalize(&geom);
        let mut rule = base_rule();
        rule.self_cond.matter = Bounds {
            min: 2.0,
            max: f32::INFINITY,
        };
        rule.other_cond.matter = Bounds {
            min: f32::NEG_INFINITY,
            max: 2.0,
        };
        rule.absorb = true;
        let rules = RuleSet { rules: vec![rule] };
        apply(
            &mut s,
            &mut bonds_scratch(),
            &geom,
            &rules,
            7,
            0,
            &mut 100u64,
            f32::INFINITY,
        );
        assert_eq!(s.len(), 2, "victim absorbed exactly once");
        let mt: f32 = s.matter.iter().sum();
        let et: f32 = s.energy.iter().sum();
        assert!((mt - 7.0).abs() < 1e-6, "double absorb would break totals");
        assert!((et - 0.5).abs() < 1e-6);
        assert_eq!(
            s.matter.iter().filter(|&&m| m == 3.0).count(),
            1,
            "exactly one initiator gained the victim's matter"
        );
    }

    #[test]
    fn copy_noise_is_deterministic_and_bounded() {
        let run = || {
            let (mut s, geom) = copy_setup();
            let rules = RuleSet {
                rules: vec![copy_rule(0.0, 0.2)],
            };
            apply(
                &mut s,
                &mut bonds_scratch(),
                &geom,
                &rules,
                7,
                0,
                &mut 1_000_000u64,
                f32::INFINITY,
            );
            let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
            s.information[at(1)]
        };
        let a = run();
        let b = run();
        assert_eq!(a, b, "noise must be a pure function of (tick, ids, rule)");
        assert!(
            (0.8 * 0.8..=0.8 * 1.2).contains(&a),
            "noisy copy outside +/-20% band: {a}"
        );
    }
}
