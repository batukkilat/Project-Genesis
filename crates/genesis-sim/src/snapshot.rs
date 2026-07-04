//! Canonical, ordered snapshot of the full simulation state.
//!
//! The snapshot is the one representation shared by hashing, save/load, and
//! (later) the Observer and renderer. Particles are always sorted by id, so
//! the same state always produces the same bytes and the same hash.

use genesis_core::StateHasher;

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
    pub dt: f32,
    pub world_width: f32,
    pub world_height: f32,
    /// Sorted by id ascending.
    pub particles: Vec<ParticleSnap>,
}

impl WorldSnapshot {
    /// Canonical state hash. Covers everything that affects future ticks.
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u64(self.tick);
        h.write_u64(self.rng_state);
        h.write_u64(self.rng_gamma);
        h.write_u64(self.next_id);
        h.write_f32(self.dt);
        h.write_f32(self.world_width);
        h.write_f32(self.world_height);
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
        h.finish()
    }
}
