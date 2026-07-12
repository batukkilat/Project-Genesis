//! Canonical, ordered snapshot of the full simulation state.
//!
//! The snapshot is the one representation shared by hashing, save/load, and
//! (later) the Observer and renderer. Particles are always sorted by id, so
//! the same state always produces the same bytes and the same hash.

use genesis_config::{ActionKind, FieldDynamics, LodPolicy, PlayerAction};
use genesis_core::StateHasher;

use crate::interact::CompiledRule;

/// One bond in canonical form: endpoint ids with `a < b`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondSnap {
    pub a: u64,
    pub b: u64,
    pub strength: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleSnap {
    pub id: u64,
    pub pos_x: f32,
    pub pos_y: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub matter: f32,
    pub energy: f32,
    pub information: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldSnapshot {
    pub tick: u64,
    pub rng_state: u64,
    pub rng_gamma: u64,
    pub next_id: u64,
    /// Base seed for derived (named) RNG streams.
    pub stream_seed: u64,
    pub dt: f32,
    pub world_width: f32,
    pub world_height: f32,
    // Physics parameters are part of replay identity: resuming a save must
    // reproduce the exact same forces.
    pub interaction_radius: f32,
    pub core_frac: f32,
    pub repulsion: f32,
    pub attraction: f32,
    pub bond_rest_length: f32,
    pub information_decay: f32,
    pub information_max: f32,
    /// Frame spin (Q-2026-07-10-B). Part of replay identity only when
    /// non-zero (a spin-0 world is byte-identical to a pre-spin world, so it
    /// must keep its exact hash — see `state_hash`).
    pub spin: f32,
    /// Adaptive-detail policy. Part of replay identity only when enabled (a
    /// disabled policy has no effect, so it must not perturb the hash — see
    /// `state_hash`).
    pub lod: LodPolicy,
    /// Environment grid dims. Meaningful only when `env_fields` is non-empty.
    pub env_cols: u32,
    pub env_rows: u32,
    /// Environment field cell values, one row-major grid per field. Fields
    /// are state: part of replay identity when any are declared, contributing
    /// nothing when empty (Q-2026-07-08-A, the LOD precedent).
    pub env_fields: Vec<Vec<f32>>,
    /// Per-field dynamics params, index-aligned with `env_fields`
    /// (Q-2026-07-08-C). They shape every future tick, so they are replay
    /// identity — but only when some field actually evolves; an all-static
    /// env contributes nothing and keeps its pre-dynamics identity.
    pub env_dynamics: Vec<FieldDynamics>,
    /// Player actions not yet applied, sorted by stamped tick
    /// (Q-2026-07-08-B). Part of replay identity when non-empty — two runs
    /// with identical current state but different pending edits have
    /// different futures. Applied actions contribute nothing here; their
    /// effect lives in `env_fields`.
    pub pending_actions: Vec<PlayerAction>,
    /// Active interaction rules — content, and therefore replay identity.
    pub rules: Vec<CompiledRule>,
    /// Sorted by id ascending.
    pub particles: Vec<ParticleSnap>,
    /// Sorted by (a, b) ascending, a < b.
    pub bonds: Vec<BondSnap>,
}

impl WorldSnapshot {
    /// Canonical state hash. Covers everything that affects future ticks.
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u64(self.tick);
        h.write_u64(self.rng_state);
        h.write_u64(self.rng_gamma);
        h.write_u64(self.next_id);
        h.write_u64(self.stream_seed);
        h.write_f32(self.dt);
        h.write_f32(self.world_width);
        h.write_f32(self.world_height);
        h.write_f32(self.interaction_radius);
        h.write_f32(self.core_frac);
        h.write_f32(self.repulsion);
        h.write_f32(self.attraction);
        h.write_f32(self.bond_rest_length);
        h.write_f32(self.information_decay);
        h.write_f32(self.information_max);
        // Frame spin enters replay identity only when non-zero: a spin-0
        // world simulates byte-identically to one that predates the param,
        // so it must hash identically (the LOD/env precedent). The tag keeps
        // the block distinct from the other conditional blocks.
        if self.spin != 0.0 {
            h.write_u64(6);
            h.write_f32(self.spin);
        }
        // The adaptive-detail policy enters replay identity only when it
        // changes the universe. A disabled policy has no effect, so it
        // contributes nothing: a LOD-off run keeps the exact hash it had
        // before LOD existed, and two cosmetically-different disabled policies
        // (which produce byte-identical simulations) hash alike. An enabled
        // policy is a different universe and hashes distinctly.
        if self.lod.enabled {
            h.write_u64(1);
            h.write_u64(self.lod.chunk_cells as u64);
            h.write_u64(self.lod.ladder.len() as u64);
            for rung in &self.lod.ladder {
                h.write_f32(rung.min_activity);
                h.write_u64(rung.rate as u64);
            }
        }
        // Environment fields enter replay identity only when declared: a world
        // with no fields is byte-identical to one that predates them, so it
        // must hash identically (Q-2026-07-08-A). The leading tag keeps this
        // conditional block from colliding with the LOD block above.
        if !self.env_fields.is_empty() {
            h.write_u64(2);
            h.write_u64(self.env_cols as u64);
            h.write_u64(self.env_rows as u64);
            h.write_u64(self.env_fields.len() as u64);
            for field in &self.env_fields {
                for &v in field {
                    h.write_f32(v);
                }
            }
        }
        // Field dynamics params enter replay identity only when some field
        // actually evolves (Q-2026-07-08-C): a static env is byte-identical
        // to a pre-dynamics one, so it must hash identically. When any field
        // is dynamic, every field's params are written so indices stay
        // unambiguous.
        if self.env_dynamics.iter().any(|d| !d.is_static()) {
            h.write_u64(5);
            h.write_u64(self.env_dynamics.len() as u64);
            for d in &self.env_dynamics {
                h.write_f32(d.diffusion);
                h.write_f32(d.relax_rate);
                h.write_f32(d.relax_to);
            }
        }
        // Pending player actions enter replay identity only while queued: an
        // action-free run keeps its exact identity, and an applied action is
        // already covered by the env cell values above (Q-2026-07-08-B).
        if !self.pending_actions.is_empty() {
            h.write_u64(4);
            h.write_u64(self.pending_actions.len() as u64);
            for a in &self.pending_actions {
                h.write_u64(a.tick);
                match a.action {
                    ActionKind::FieldSet {
                        field,
                        region,
                        value,
                    }
                    | ActionKind::FieldAdd {
                        field,
                        region,
                        delta: value,
                    } => {
                        let code = match a.action {
                            ActionKind::FieldSet { .. } => 0u64,
                            _ => 1u64,
                        };
                        h.write_u64(code);
                        h.write_u64(field as u64);
                        h.write_f32(region.x0);
                        h.write_f32(region.y0);
                        h.write_f32(region.x1);
                        h.write_f32(region.y1);
                        h.write_f32(value);
                    }
                    // A pending impact is a different future, exactly like a
                    // pending field edit; every parameter shapes the outcome,
                    // so every parameter hashes.
                    ActionKind::Impact {
                        x,
                        y,
                        radius,
                        impulse,
                        energy,
                        payload,
                    } => {
                        h.write_u64(2);
                        h.write_f32(x);
                        h.write_f32(y);
                        h.write_f32(radius);
                        h.write_f32(impulse);
                        h.write_f32(energy);
                        h.write_u64(payload.count as u64);
                        for r in [
                            payload.matter,
                            payload.energy,
                            payload.information,
                            payload.speed,
                        ] {
                            h.write_f32(r.lo);
                            h.write_f32(r.hi);
                        }
                        h.write_f32(payload.spread);
                    }
                    // A pending rift, likewise (Q-2026-07-10-C).
                    ActionKind::Rift {
                        x0,
                        y0,
                        x1,
                        y1,
                        radius,
                        impulse,
                        energy,
                        payload,
                    } => {
                        h.write_u64(3);
                        h.write_f32(x0);
                        h.write_f32(y0);
                        h.write_f32(x1);
                        h.write_f32(y1);
                        h.write_f32(radius);
                        h.write_f32(impulse);
                        h.write_f32(energy);
                        h.write_u64(payload.count as u64);
                        for r in [
                            payload.matter,
                            payload.energy,
                            payload.information,
                            payload.speed,
                        ] {
                            h.write_f32(r.lo);
                            h.write_f32(r.hi);
                        }
                        h.write_f32(payload.spread);
                    }
                    // A pending spin change is a different future
                    // (Q-2026-07-10-B); once applied, the spin param above
                    // carries it.
                    ActionKind::SpinSet { spin } => {
                        h.write_u64(4);
                        h.write_f32(spin);
                    }
                }
            }
        }
        h.write_u64(self.rules.len() as u64);
        for rule in &self.rules {
            for v in rule.fields() {
                h.write_f32(v);
            }
            // Non-empty env gates only: an env-free rule behaves identically
            // to a pre-env rule, so it must hash identically.
            rule.hash_env_into(&mut h);
        }
        h.write_u64(self.particles.len() as u64);
        for p in &self.particles {
            h.write_u64(p.id);
            h.write_f32(p.pos_x);
            h.write_f32(p.pos_y);
            h.write_f32(p.vel_x);
            h.write_f32(p.vel_y);
            h.write_f32(p.matter);
            h.write_f32(p.energy);
            h.write_f32(p.information);
        }
        h.write_u64(self.bonds.len() as u64);
        for b in &self.bonds {
            h.write_u64(b.a);
            h.write_u64(b.b);
            h.write_f32(b.strength);
        }
        h.finish()
    }
}
