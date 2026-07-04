//! The simulation core: ECS world, fixed-timestep loop, deterministic spawn.
//!
//! Runs fully headless. Rendering, observation, and AI live in other crates
//! and only ever consume snapshots — nothing outside this crate mutates the
//! world. Phase 1 particles are inert: the loop, RNG, and state plumbing are
//! real; motion and interactions arrive in later phases.

pub mod components;
pub mod snapshot;

use bevy_ecs::prelude::*;
use genesis_config::SimConfig;
use genesis_core::{DetRng, ParticleId, Vec2};

use components::{Energy, Information, Matter, Pid, Position, Velocity};
use snapshot::{ParticleSnap, WorldSnapshot};

/// Simulation tick counter. Time inside the simulation is this counter and
/// nothing else — the wall clock is never consulted.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick(pub u64);

/// The simulation's master RNG stream. All randomness derives from it; systems
/// split child streams rather than sharing this one.
#[derive(Resource, Debug, Clone)]
pub struct SimRng(pub DetRng);

/// Fixed world parameters, set at creation from config.
#[derive(Resource, Debug, Clone, Copy)]
pub struct Params {
    /// Simulated seconds per tick (fixed timestep).
    pub dt: f32,
    pub world_width: f32,
    pub world_height: f32,
}

/// Next particle id to assign. Ids are sequential and never reused.
#[derive(Resource, Debug, Clone, Copy)]
pub struct NextId(pub u64);

fn advance_tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

pub struct Simulation {
    world: World,
    schedule: Schedule,
}

impl Simulation {
    /// Fresh simulation: spawns `config.particle_count` particles using RNG
    /// streams derived from `config.seed`. The same validated config always
    /// produces the same world.
    pub fn new(config: &SimConfig) -> Self {
        let mut master = DetRng::new(config.seed);
        let mut spawn_rng = master.split();

        let mut sim = Self::empty(
            Params {
                dt: config.dt(),
                world_width: config.world_width,
                world_height: config.world_height,
            },
            Tick(0),
            SimRng(master),
            NextId(0),
        );

        for _ in 0..config.particle_count {
            let position = Vec2::new(
                spawn_rng.range_f32(0.0, config.world_width),
                spawn_rng.range_f32(0.0, config.world_height),
            );
            let speed = spawn_rng.range_f32(config.initial.speed.lo, config.initial.speed.hi);
            let angle = spawn_rng.range_f32(0.0, std::f32::consts::TAU);
            let velocity = Vec2::new(angle.cos() * speed, angle.sin() * speed);
            let matter = spawn_rng.range_f32(config.initial.matter.lo, config.initial.matter.hi);
            let energy = spawn_rng.range_f32(config.initial.energy.lo, config.initial.energy.hi);
            let information =
                spawn_rng.range_f32(config.initial.information.lo, config.initial.information.hi);

            let id = {
                let mut next = sim.world.resource_mut::<NextId>();
                let id = next.0;
                next.0 += 1;
                ParticleId(id)
            };
            sim.world.spawn((
                Pid(id),
                Position(position),
                Velocity(velocity),
                Matter(matter),
                Energy(energy),
                Information(information),
            ));
        }

        tracing::info!(
            particles = config.particle_count,
            seed = config.seed,
            "simulation created"
        );
        sim
    }

    /// Rebuild a simulation from a snapshot (load path). Continuing from the
    /// snapshot is byte-identical to a run that was never saved.
    pub fn from_snapshot(snap: &WorldSnapshot) -> Self {
        let mut sim = Self::empty(
            Params {
                dt: snap.dt,
                world_width: snap.world_width,
                world_height: snap.world_height,
            },
            Tick(snap.tick),
            SimRng(DetRng::from_parts(snap.rng_state, snap.rng_gamma)),
            NextId(snap.next_id),
        );
        for p in &snap.particles {
            sim.world.spawn((
                Pid(ParticleId(p.id)),
                Position(Vec2::new(p.pos_x, p.pos_y)),
                Velocity(Vec2::new(p.vel_x, p.vel_y)),
                Matter(p.matter),
                Energy(p.energy),
                Information(p.information),
            ));
        }
        sim
    }

    fn empty(params: Params, tick: Tick, rng: SimRng, next_id: NextId) -> Self {
        let mut world = World::new();
        world.insert_resource(params);
        world.insert_resource(tick);
        world.insert_resource(rng);
        world.insert_resource(next_id);

        let mut schedule = Schedule::default();
        // Phase 1: the loop only advances time. Physics (Phase 2) and the
        // interaction system (Phase 3) register their systems here.
        schedule.add_systems(advance_tick);

        Simulation { world, schedule }
    }

    /// Advance the simulation by exactly one fixed timestep.
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
    }

    pub fn tick_count(&self) -> u64 {
        self.world.resource::<Tick>().0
    }

    /// Canonical snapshot of the full simulation state, particles sorted by id.
    pub fn snapshot(&mut self) -> WorldSnapshot {
        let params = *self.world.resource::<Params>();
        let tick = self.world.resource::<Tick>().0;
        let (rng_state, rng_gamma) = self.world.resource::<SimRng>().0.to_parts();
        let next_id = self.world.resource::<NextId>().0;

        let mut particles: Vec<ParticleSnap> = self
            .world
            .query::<(&Pid, &Position, &Velocity, &Matter, &Energy, &Information)>()
            .iter(&self.world)
            .map(|(pid, pos, vel, m, e, i)| ParticleSnap {
                id: pid.0.0,
                pos_x: pos.0.x,
                pos_y: pos.0.y,
                vel_x: vel.0.x,
                vel_y: vel.0.y,
                matter: m.0,
                energy: e.0,
                information: i.0,
            })
            .collect();
        particles.sort_unstable_by_key(|p| p.id);

        WorldSnapshot {
            tick,
            rng_state,
            rng_gamma,
            next_id,
            dt: params.dt,
            world_width: params.world_width,
            world_height: params.world_height,
            particles,
        }
    }

    /// Canonical hash of the current state. Equal hashes across runs is the
    /// project's definition of deterministic replay.
    pub fn state_hash(&mut self) -> u64 {
        self.snapshot().state_hash()
    }

    pub fn particle_count(&mut self) -> usize {
        self.world.query::<&Pid>().iter(&self.world).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SimConfig {
        SimConfig {
            seed: 42,
            particle_count: 500,
            ..Default::default()
        }
    }

    #[test]
    fn spawn_count_matches_config() {
        let mut sim = Simulation::new(&test_config());
        assert_eq!(sim.particle_count(), 500);
    }

    #[test]
    fn same_config_same_hash() {
        let mut a = Simulation::new(&test_config());
        let mut b = Simulation::new(&test_config());
        assert_eq!(a.state_hash(), b.state_hash());
        for _ in 0..100 {
            a.tick();
            b.tick();
        }
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(a.tick_count(), 100);
    }

    #[test]
    fn different_seed_different_hash() {
        let mut a = Simulation::new(&test_config());
        let mut b = Simulation::new(&SimConfig {
            seed: 43,
            ..test_config()
        });
        assert_ne!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn tick_changes_hash() {
        let mut sim = Simulation::new(&test_config());
        let before = sim.state_hash();
        sim.tick();
        assert_ne!(before, sim.state_hash());
    }

    #[test]
    fn snapshot_roundtrip_preserves_state() {
        let mut sim = Simulation::new(&test_config());
        for _ in 0..10 {
            sim.tick();
        }
        let snap = sim.snapshot();
        let mut restored = Simulation::from_snapshot(&snap);
        assert_eq!(sim.state_hash(), restored.state_hash());

        // Continuing after restore matches an uninterrupted run.
        for _ in 0..50 {
            sim.tick();
            restored.tick();
        }
        assert_eq!(sim.state_hash(), restored.state_hash());
    }
}
