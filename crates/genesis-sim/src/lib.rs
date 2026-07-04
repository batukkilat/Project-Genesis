//! The simulation core: fixed-timestep loop, deterministic spawn, SoA
//! particle storage, and the Phase 2 physics pass (short-range kernel forces
//! on a torus, chunk-parallel).
//!
//! Runs fully headless. Rendering, observation, and AI live in other crates
//! and only ever consume snapshots — nothing outside this crate mutates the
//! world. Bevy ECS hosts the schedule and resources; particle data itself
//! lives in a SoA store (`store.rs`) for cache-friendly, deterministic
//! parallel iteration.

pub mod grid;
pub mod physics;
pub mod snapshot;
pub mod store;

use bevy_ecs::prelude::*;
use genesis_config::{PhysicsParams, SimConfig};
use genesis_core::DetRng;

use grid::GridGeom;
use snapshot::{ParticleSnap, WorldSnapshot};
use store::ParticleStore;

/// Simulation tick counter. Time inside the simulation is this counter and
/// nothing else — the wall clock is never consulted.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick(pub u64);

/// The simulation's master RNG stream. All randomness derives from it; systems
/// split child streams rather than sharing this one. (Physics uses no RNG.)
#[derive(Resource, Debug, Clone)]
pub struct SimRng(pub DetRng);

/// Fixed world parameters, set at creation from config.
#[derive(Resource, Debug, Clone, Copy)]
pub struct Params {
    /// Simulated seconds per tick (fixed timestep).
    pub dt: f32,
    pub world_width: f32,
    pub world_height: f32,
    pub physics: PhysicsParams,
}

/// Next particle id to assign. Ids are sequential and never reused.
#[derive(Resource, Debug, Clone, Copy)]
pub struct NextId(pub u64);

fn advance_tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

/// One physics step: canonicalize layout, compute kernel forces, integrate.
/// Layout is re-derived from state at the start of every tick, so force
/// accumulation order is a pure function of state (see `store.rs`).
fn physics_step(mut store: ResMut<ParticleStore>, geom: Res<GridGeom>, params: Res<Params>) {
    store.canonicalize(&geom);
    physics::forces(&mut store, &geom, &params.physics);
    physics::integrate(&mut store, &geom, params.dt);
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

        let params = Params {
            dt: config.dt(),
            world_width: config.world_width,
            world_height: config.world_height,
            physics: config.physics,
        };

        let mut store = ParticleStore::default();
        for id in 0..config.particle_count {
            let x = spawn_rng.range_f32(0.0, config.world_width);
            let y = spawn_rng.range_f32(0.0, config.world_height);
            let speed = spawn_rng.range_f32(config.initial.speed.lo, config.initial.speed.hi);
            let angle = spawn_rng.range_f32(0.0, std::f32::consts::TAU);
            let matter = spawn_rng.range_f32(config.initial.matter.lo, config.initial.matter.hi);
            let energy = spawn_rng.range_f32(config.initial.energy.lo, config.initial.energy.hi);
            let information =
                spawn_rng.range_f32(config.initial.information.lo, config.initial.information.hi);
            store.push(
                id,
                x,
                y,
                angle.cos() * speed,
                angle.sin() * speed,
                matter,
                energy,
                information,
            );
        }

        let sim = Self::assemble(
            params,
            Tick(0),
            SimRng(master),
            NextId(config.particle_count),
            store,
        );
        tracing::info!(
            particles = config.particle_count,
            seed = config.seed,
            "simulation created"
        );
        sim
    }

    /// Rebuild a simulation from a snapshot (load path). Continuing from the
    /// snapshot is byte-identical to a run that was never saved, because the
    /// particle layout is re-canonicalized from state at the next tick.
    pub fn from_snapshot(snap: &WorldSnapshot) -> Self {
        let params = Params {
            dt: snap.dt,
            world_width: snap.world_width,
            world_height: snap.world_height,
            physics: PhysicsParams {
                interaction_radius: snap.interaction_radius,
                core_frac: snap.core_frac,
                repulsion: snap.repulsion,
                attraction: snap.attraction,
            },
        };
        let mut store = ParticleStore::default();
        for p in &snap.particles {
            store.push(
                p.id,
                p.pos_x,
                p.pos_y,
                p.vel_x,
                p.vel_y,
                p.matter,
                p.energy,
                p.information,
            );
        }
        Self::assemble(
            params,
            Tick(snap.tick),
            SimRng(DetRng::from_parts(snap.rng_state, snap.rng_gamma)),
            NextId(snap.next_id),
            store,
        )
    }

    fn assemble(
        params: Params,
        tick: Tick,
        rng: SimRng,
        next_id: NextId,
        store: ParticleStore,
    ) -> Self {
        let mut world = World::new();
        world.insert_resource(GridGeom::new(
            params.world_width,
            params.world_height,
            params.physics.interaction_radius,
        ));
        world.insert_resource(params);
        world.insert_resource(tick);
        world.insert_resource(rng);
        world.insert_resource(next_id);
        world.insert_resource(store);

        let mut schedule = Schedule::default();
        // Phase 2: physics, then time. The interaction system (Phase 3)
        // slots between them.
        schedule.add_systems((physics_step, advance_tick).chain());

        Simulation { world, schedule }
    }

    /// Advance the simulation by exactly one fixed timestep.
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
    }

    pub fn tick_count(&self) -> u64 {
        self.world.resource::<Tick>().0
    }

    pub fn particle_count(&self) -> usize {
        self.world.resource::<ParticleStore>().len()
    }

    /// Canonical snapshot of the full simulation state, particles sorted by id.
    pub fn snapshot(&self) -> WorldSnapshot {
        let params = *self.world.resource::<Params>();
        let tick = self.world.resource::<Tick>().0;
        let (rng_state, rng_gamma) = self.world.resource::<SimRng>().0.to_parts();
        let next_id = self.world.resource::<NextId>().0;
        let store = self.world.resource::<ParticleStore>();

        let mut particles: Vec<ParticleSnap> = (0..store.len())
            .map(|i| ParticleSnap {
                id: store.id[i],
                pos_x: store.px[i],
                pos_y: store.py[i],
                vel_x: store.vx[i],
                vel_y: store.vy[i],
                matter: store.matter[i],
                energy: store.energy[i],
                information: store.information[i],
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
            interaction_radius: params.physics.interaction_radius,
            core_frac: params.physics.core_frac,
            repulsion: params.physics.repulsion,
            attraction: params.physics.attraction,
            particles,
        }
    }

    /// Canonical hash of the current state. Equal hashes across runs is the
    /// project's definition of deterministic replay.
    pub fn state_hash(&self) -> u64 {
        self.snapshot().state_hash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SimConfig {
        SimConfig {
            seed: 42,
            particle_count: 500,
            world_width: 256.0,
            world_height: 256.0,
            ..Default::default()
        }
    }

    #[test]
    fn spawn_count_matches_config() {
        let sim = Simulation::new(&test_config());
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
        let a = Simulation::new(&test_config());
        let b = Simulation::new(&SimConfig {
            seed: 43,
            ..test_config()
        });
        assert_ne!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn particles_actually_move() {
        let mut sim = Simulation::new(&test_config());
        let before = sim.snapshot();
        for _ in 0..10 {
            sim.tick();
        }
        let after = sim.snapshot();
        let moved = before
            .particles
            .iter()
            .zip(&after.particles)
            .filter(|(a, b)| a.pos_x != b.pos_x || a.pos_y != b.pos_y)
            .count();
        assert!(moved > 400, "expected most particles to move, got {moved}");
    }

    #[test]
    fn positions_stay_in_bounds() {
        let mut sim = Simulation::new(&test_config());
        for _ in 0..200 {
            sim.tick();
        }
        let snap = sim.snapshot();
        for p in &snap.particles {
            assert!(
                (0.0..256.0).contains(&p.pos_x),
                "x out of bounds: {}",
                p.pos_x
            );
            assert!(
                (0.0..256.0).contains(&p.pos_y),
                "y out of bounds: {}",
                p.pos_y
            );
        }
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

    #[test]
    fn thread_count_does_not_change_hash() {
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut sim = Simulation::new(&test_config());
                for _ in 0..100 {
                    sim.tick();
                }
                sim.state_hash()
            })
        };
        let single = run(1);
        let multi = run(4);
        assert_eq!(
            single, multi,
            "thread count changed the simulation — determinism broken"
        );
    }

    #[test]
    fn matter_is_conserved_exactly() {
        let mut sim = Simulation::new(&test_config());
        let total = |s: &WorldSnapshot| s.particles.iter().map(|p| p.matter as f64).sum::<f64>();
        let before = total(&sim.snapshot());
        for _ in 0..500 {
            sim.tick();
        }
        let after = total(&sim.snapshot());
        // Snapshot order is id-sorted, so the f64 sums are bit-comparable.
        assert_eq!(before, after);
    }

    #[test]
    fn momentum_is_conserved() {
        let mut sim = Simulation::new(&test_config());
        let momentum = |s: &WorldSnapshot| {
            s.particles.iter().fold((0.0f64, 0.0f64), |(px, py), p| {
                (
                    px + p.matter as f64 * p.vel_x as f64,
                    py + p.matter as f64 * p.vel_y as f64,
                )
            })
        };
        let scale: f64 = sim
            .snapshot()
            .particles
            .iter()
            .map(|p| (p.matter as f64) * (p.vel_x.abs() + p.vel_y.abs()) as f64)
            .sum::<f64>()
            .max(1.0);
        let (px0, py0) = momentum(&sim.snapshot());
        for _ in 0..500 {
            sim.tick();
        }
        let (px1, py1) = momentum(&sim.snapshot());
        assert!(
            ((px1 - px0).abs() + (py1 - py0).abs()) / scale < 1e-3,
            "momentum drifted: ({px0}, {py0}) -> ({px1}, {py1})"
        );
    }

    #[test]
    fn energy_drift_is_bounded() {
        // Gentle, conservative configuration: kinetic + pair potential should
        // drift only through integration error, not leak.
        let mut config = test_config();
        config.initial.speed = genesis_config::Range::new(0.0, 1.0);
        config.initial.matter = genesis_config::Range::new(0.5, 1.0);
        config.physics.repulsion = 20.0;
        config.physics.attraction = 2.0;
        let params = config.physics;
        let (w, h) = (config.world_width, config.world_height);

        let total_energy = |s: &WorldSnapshot| {
            let kinetic: f64 = s
                .particles
                .iter()
                .map(|p| {
                    0.5 * p.matter as f64
                        * (p.vel_x as f64 * p.vel_x as f64 + p.vel_y as f64 * p.vel_y as f64)
                })
                .sum();
            let mut pot = 0.0f64;
            for i in 0..s.particles.len() {
                for j in (i + 1)..s.particles.len() {
                    let a = &s.particles[i];
                    let b = &s.particles[j];
                    let dx = genesis_core::torus::delta(a.pos_x, b.pos_x, w);
                    let dy = genesis_core::torus::delta(a.pos_y, b.pos_y, h);
                    let r = (dx * dx + dy * dy).sqrt();
                    if r < params.interaction_radius {
                        pot += physics::potential(r, &params) as f64;
                    }
                }
            }
            kinetic + pot
        };

        let mut sim = Simulation::new(&config);
        let e0 = total_energy(&sim.snapshot());
        for _ in 0..1000 {
            sim.tick();
        }
        let e1 = total_energy(&sim.snapshot());
        let scale = e0.abs().max(1.0);
        assert!(
            ((e1 - e0) / scale).abs() < 0.05,
            "energy drifted more than 5%: {e0} -> {e1}"
        );
    }
}
