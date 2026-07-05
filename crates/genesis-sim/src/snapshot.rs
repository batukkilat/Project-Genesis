//! Canonical, ordered snapshot of the full simulation state.
//!
//! The snapshot is the one representation shared by hashing, save/load, and
//! (later) the Observer and renderer. Particles are always sorted by id, so
//! the same state always produces the same bytes and the same hash.

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
        h.write_u64(self.rules.len() as u64);
        for rule in &self.rules {
            for v in rule.fields() {
                h.write_f32(v);
            }
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
