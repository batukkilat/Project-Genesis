//! The simulation core: fixed-timestep loop, deterministic spawn, SoA
//! particle storage, and the Phase 2 physics pass (short-range kernel forces
//! on a torus, chunk-parallel).
//!
//! Runs fully headless. Rendering, observation, and AI live in other crates
//! and only ever consume snapshots — nothing outside this crate mutates the
//! world. Bevy ECS hosts the schedule and resources; particle data itself
//! lives in a SoA store (`store.rs`) for cache-friendly, deterministic
//! parallel iteration.

pub mod bonds;
pub mod grid;
pub mod interact;
pub mod physics;
pub mod snapshot;
pub mod store;

use bevy_ecs::prelude::*;
use genesis_config::{PhysicsParams, SimConfig};
use genesis_core::DetRng;

use bonds::BondStore;
use grid::GridGeom;
use interact::RuleSet;
use snapshot::{BondSnap, ParticleSnap, WorldSnapshot};
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

/// Base seed for order-free derived RNG streams (`DetRng::derive`). Drawn
/// once from the master stream at creation; part of replay identity.
#[derive(Resource, Debug, Clone, Copy)]
pub struct StreamSeed(pub u64);

fn advance_tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

/// One simulation step: canonicalize layout, compute kernel forces, run
/// discrete interactions, integrate. Layout is re-derived from state at the
/// start of every tick, so iteration order is a pure function of state
/// (see `store.rs`). Interactions run before integration because intents
/// hold layout indices, which position changes would not invalidate — but
/// the pair distances must match the positions the conditions saw.
#[allow(clippy::too_many_arguments)]
fn sim_step(
    mut store: ResMut<ParticleStore>,
    mut bonds: ResMut<BondStore>,
    geom: Res<GridGeom>,
    params: Res<Params>,
    rules: Res<RuleSet>,
    stream_seed: Res<StreamSeed>,
    tick: Res<Tick>,
    mut next_id: ResMut<NextId>,
) {
    store.canonicalize(&geom);
    bonds.rebuild(&store);
    physics::forces(&mut store, &geom, &params.physics, &bonds);
    interact::apply(
        &mut store,
        &mut bonds,
        &geom,
        &rules,
        stream_seed.0,
        tick.0,
        &mut next_id.0,
        params.physics.information_max,
    );
    physics::integrate(&mut store, &geom, &params.physics, params.dt);
}

pub struct Simulation {
    world: World,
    schedule: Schedule,
}

impl Simulation {
    /// Fresh simulation with no interaction rules.
    pub fn new(config: &SimConfig) -> Self {
        Self::with_rules(config, RuleSet::default())
    }

    /// Fresh simulation: spawns `config.particle_count` particles using RNG
    /// streams derived from `config.seed`. The same validated config and
    /// rule set always produce the same world.
    pub fn with_rules(config: &SimConfig, rules: RuleSet) -> Self {
        let mut master = DetRng::new(config.seed);
        let mut spawn_rng = master.split();
        let stream_seed = master.next_u64();

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
            StreamSeed(stream_seed),
            rules,
            store,
            BondStore::default(),
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
                bond_rest_length: snap.bond_rest_length,
                information_decay: snap.information_decay,
                information_max: snap.information_max,
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
        let mut bonds = BondStore::default();
        for b in &snap.bonds {
            bonds.push(b.a, b.b, b.strength);
        }
        bonds.sort_canonical();
        Self::assemble(
            params,
            Tick(snap.tick),
            SimRng(DetRng::from_parts(snap.rng_state, snap.rng_gamma)),
            NextId(snap.next_id),
            StreamSeed(snap.stream_seed),
            RuleSet {
                rules: snap.rules.clone(),
            },
            store,
            bonds,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        params: Params,
        tick: Tick,
        rng: SimRng,
        next_id: NextId,
        stream_seed: StreamSeed,
        rules: RuleSet,
        store: ParticleStore,
        bonds: BondStore,
    ) -> Self {
        rules.assert_valid(params.physics.interaction_radius);
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
        world.insert_resource(stream_seed);
        world.insert_resource(rules);
        world.insert_resource(store);
        world.insert_resource(bonds);

        let mut schedule = Schedule::default();
        schedule.add_systems((sim_step, advance_tick).chain());

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
        let stream_seed = self.world.resource::<StreamSeed>().0;
        let rules = self.world.resource::<RuleSet>().rules.clone();
        let store = self.world.resource::<ParticleStore>();
        let bond_store = self.world.resource::<BondStore>();

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

        // The edge list is kept sorted by (a, b) at all times, so this is
        // already canonical.
        let bonds: Vec<BondSnap> = (0..bond_store.len())
            .map(|r| BondSnap {
                a: bond_store.a[r],
                b: bond_store.b[r],
                strength: bond_store.strength[r],
            })
            .collect();

        WorldSnapshot {
            tick,
            rng_state,
            rng_gamma,
            next_id,
            stream_seed,
            dt: params.dt,
            world_width: params.world_width,
            world_height: params.world_height,
            interaction_radius: params.physics.interaction_radius,
            core_frac: params.physics.core_frac,
            repulsion: params.physics.repulsion,
            attraction: params.physics.attraction,
            bond_rest_length: params.physics.bond_rest_length,
            information_decay: params.physics.information_decay,
            information_max: params.physics.information_max,
            rules,
            particles,
            bonds,
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
        let mut config = SimConfig {
            seed: 42,
            particle_count: 500,
            world_width: 256.0,
            world_height: 256.0,
            ..Default::default()
        };
        // Give particles information so its conservation is actually
        // exercised, not vacuously true on all-zeros.
        config.initial.information = genesis_config::Range::new(0.0, 1.0);
        config
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

    fn test_rules() -> RuleSet {
        RuleSet {
            rules: vec![interact::CompiledRule {
                radius: 8.0,
                self_cond: interact::QuantityCondition::ANY,
                other_cond: interact::QuantityCondition::ANY,
                probability: 0.05,
                transfer_matter: 0.01,
                transfer_energy: 0.05,
                transfer_information: 0.01,
                bond_action: interact::BondAction::None,
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
        }
    }

    /// Transfers plus bonding, breaking, and a lossy info copy — exercises
    /// the full Phase 3 pipeline every tick.
    fn bonding_rules() -> RuleSet {
        let mut set = test_rules();
        set.rules.push(interact::CompiledRule {
            radius: 6.0,
            self_cond: interact::QuantityCondition {
                matter: interact::Bounds::ANY,
                energy: interact::Bounds::ANY,
                information: interact::Bounds {
                    min: 0.3,
                    max: f32::INFINITY,
                },
            },
            other_cond: interact::QuantityCondition::ANY,
            probability: 0.1,
            transfer_matter: 0.0,
            transfer_energy: 0.0,
            transfer_information: 0.0,
            bond_action: interact::BondAction::None,
            bond_strength: 0.0,
            info_copy: true,
            info_cost: 0.02,
            info_noise: 0.25,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
        });
        set.rules.push(interact::CompiledRule {
            radius: 4.0,
            self_cond: interact::QuantityCondition::ANY,
            other_cond: interact::QuantityCondition::ANY,
            probability: 0.2,
            transfer_matter: 0.0,
            transfer_energy: 0.0,
            transfer_information: 0.0,
            bond_action: interact::BondAction::Create,
            bond_strength: 3.0,
            info_copy: false,
            info_cost: 0.0,
            info_noise: 0.0,
            emit: false,
            emit_matter_frac: 0.0,
            emit_energy_frac: 0.0,
            emit_info_frac: 0.0,
            emit_offset: 0.0,
            absorb: false,
        });
        set.rules.push(interact::CompiledRule {
            radius: 8.0,
            self_cond: interact::QuantityCondition::ANY,
            other_cond: interact::QuantityCondition::ANY,
            probability: 0.01,
            transfer_matter: 0.0,
            transfer_energy: 0.0,
            transfer_information: 0.0,
            bond_action: interact::BondAction::Break,
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
        });
        set
    }

    #[test]
    fn bonds_form_and_are_deterministic() {
        let run = || {
            let mut sim = Simulation::with_rules(&test_config(), bonding_rules());
            for _ in 0..100 {
                sim.tick();
            }
            sim
        };
        let a = run();
        let b = run();
        let snap = a.snapshot();
        assert!(
            !snap.bonds.is_empty(),
            "bonding rule at p=0.2 must have formed bonds in 100 ticks"
        );
        // Canonical order invariant.
        for w in snap.bonds.windows(2) {
            assert!((w[0].a, w[0].b) < (w[1].a, w[1].b), "bonds not sorted");
        }
        for bond in &snap.bonds {
            assert!(bond.a < bond.b);
        }
        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn bond_thread_count_does_not_change_hash() {
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut sim = Simulation::with_rules(&test_config(), bonding_rules());
                for _ in 0..100 {
                    sim.tick();
                }
                sim.state_hash()
            })
        };
        assert_eq!(run(1), run(4), "bond pipeline broke thread invariance");
    }

    #[test]
    fn resume_with_bonds_matches_uninterrupted() {
        let mut a = Simulation::with_rules(&test_config(), bonding_rules());
        let mut b = Simulation::with_rules(&test_config(), bonding_rules());
        for _ in 0..60 {
            a.tick();
            b.tick();
        }
        let snap = b.snapshot();
        assert!(!snap.bonds.is_empty(), "test needs live bonds at the save");
        let mut resumed = Simulation::from_snapshot(&snap);
        drop(b);
        for _ in 0..60 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(a.state_hash(), resumed.state_hash());
    }

    /// Emit + absorb churn: energetic heavies split, heavies eat lights.
    fn fission_rules() -> RuleSet {
        let mut split = interact::CompiledRule {
            radius: 6.0,
            self_cond: interact::QuantityCondition::ANY,
            other_cond: interact::QuantityCondition::ANY,
            probability: 0.05,
            transfer_matter: 0.0,
            transfer_energy: 0.0,
            transfer_information: 0.0,
            bond_action: interact::BondAction::None,
            bond_strength: 0.0,
            info_copy: false,
            info_cost: 0.0,
            info_noise: 0.0,
            emit: true,
            emit_matter_frac: 0.5,
            emit_energy_frac: 0.5,
            emit_info_frac: 0.5,
            emit_offset: 1.0,
            absorb: false,
        };
        split.self_cond.matter = interact::Bounds {
            min: 0.6,
            max: f32::INFINITY,
        };
        split.self_cond.energy = interact::Bounds {
            min: 0.5,
            max: f32::INFINITY,
        };
        let mut eat = split;
        eat.emit = false;
        eat.emit_matter_frac = 0.0;
        eat.emit_energy_frac = 0.0;
        eat.emit_info_frac = 0.0;
        eat.emit_offset = 0.0;
        eat.absorb = true;
        eat.probability = 0.03;
        eat.self_cond.energy = interact::Bounds::ANY;
        eat.other_cond.matter = interact::Bounds {
            min: f32::NEG_INFINITY,
            max: 0.3,
        };
        RuleSet {
            rules: vec![split, eat],
        }
    }

    #[test]
    fn create_destroy_conserves_and_stays_deterministic() {
        let run = || {
            let mut sim = Simulation::with_rules(&test_config(), fission_rules());
            for _ in 0..150 {
                sim.tick();
            }
            sim
        };
        let a = run();
        let b = run();
        assert_eq!(a.state_hash(), b.state_hash());

        let snap = a.snapshot();
        assert_ne!(
            snap.particles.len(),
            500,
            "150 ticks of split/absorb churn should change the population"
        );
        // Matter and energy conserved through every create/destroy event.
        let m: f64 = snap.particles.iter().map(|p| p.matter as f64).sum();
        let e: f64 = snap.particles.iter().map(|p| p.energy as f64).sum();
        let fresh = Simulation::with_rules(&test_config(), fission_rules()).snapshot();
        let m0: f64 = fresh.particles.iter().map(|p| p.matter as f64).sum();
        let e0: f64 = fresh.particles.iter().map(|p| p.energy as f64).sum();
        assert!(((m - m0) / m0).abs() < 1e-4, "matter leaked: {m0} -> {m}");
        assert!(
            ((e - e0) / e0.max(1.0)).abs() < 1e-4,
            "energy leaked: {e0} -> {e}"
        );

        // Ids unique, never reused, all below the next-id watermark.
        let mut ids: Vec<u64> = snap.particles.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), snap.particles.len(), "duplicate ids");
        assert!(ids.iter().all(|&id| id < snap.next_id));
        assert!(snap.next_id > 500, "emissions must have consumed ids");
    }

    #[test]
    fn create_destroy_thread_count_does_not_change_hash() {
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut sim = Simulation::with_rules(&test_config(), fission_rules());
                for _ in 0..100 {
                    sim.tick();
                }
                sim.state_hash()
            })
        };
        assert_eq!(run(1), run(4), "create/destroy broke thread invariance");
    }

    #[test]
    fn resume_with_create_destroy_matches_uninterrupted() {
        let mut a = Simulation::with_rules(&test_config(), fission_rules());
        let mut b = Simulation::with_rules(&test_config(), fission_rules());
        for _ in 0..60 {
            a.tick();
            b.tick();
        }
        let mut resumed = Simulation::from_snapshot(&b.snapshot());
        drop(b);
        for _ in 0..60 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(a.state_hash(), resumed.state_hash());
    }

    #[test]
    fn info_copy_creates_information_and_conserves_the_rest() {
        // Under copy rules information is deliberately non-conserved (it is
        // created by paying energy) while matter and energy stay conserved.
        let mut sim = Simulation::with_rules(&test_config(), bonding_rules());
        let totals = |s: &WorldSnapshot| {
            let m: f64 = s.particles.iter().map(|p| p.matter as f64).sum();
            let e: f64 = s.particles.iter().map(|p| p.energy as f64).sum();
            let i: f64 = s.particles.iter().map(|p| p.information as f64).sum();
            (m, e, i)
        };
        let (m0, e0, i0) = totals(&sim.snapshot());
        for _ in 0..200 {
            sim.tick();
        }
        let (m1, e1, i1) = totals(&sim.snapshot());
        assert!(((m1 - m0) / m0).abs() < 1e-4, "matter leaked: {m0} -> {m1}");
        assert!(
            ((e1 - e0) / e0.max(1.0)).abs() < 1e-4,
            "energy leaked: {e0} -> {e1}"
        );
        assert!(
            (i1 - i0).abs() / i0.max(1.0) > 1e-3,
            "copy rules ran 200 ticks yet information total barely moved \
             ({i0} -> {i1}) — copy action probably never fired"
        );
    }

    #[test]
    fn information_decays_at_configured_rate() {
        let mut config = test_config();
        config.physics.information_decay = 0.5; // per second; dt = 1/60
        let mut sim = Simulation::new(&config);
        let before = sim.snapshot();
        let ticks = 30;
        for _ in 0..ticks {
            sim.tick();
        }
        let after = sim.snapshot();
        // Physics without rules never touches information except decay, so
        // each particle's value is exactly `info * factor^ticks` in f32.
        let factor = 1.0f32 - 0.5 * config.dt();
        for (b, a) in before.particles.iter().zip(&after.particles) {
            let mut expect = b.information;
            for _ in 0..ticks {
                expect *= factor;
            }
            assert_eq!(a.information, expect, "particle {}", b.id);
        }

        // Decay is replay identity: resume mid-decay stays bit-identical.
        let mut resumed = Simulation::from_snapshot(&sim.snapshot());
        let mut uninterrupted = sim;
        for _ in 0..30 {
            uninterrupted.tick();
            resumed.tick();
        }
        assert_eq!(uninterrupted.state_hash(), resumed.state_hash());
    }

    #[test]
    fn bonds_are_replay_identity() {
        // Same particles, one extra bond: different hash from tick 0.
        let mut a = Simulation::with_rules(&test_config(), test_rules());
        let snap = a.snapshot();
        let mut with_bond = snap.clone();
        with_bond.bonds.push(snapshot::BondSnap {
            a: 0,
            b: 1,
            strength: 1.0,
        });
        assert_ne!(snap.state_hash(), with_bond.state_hash());

        // And the bond changes the future: the spring does work.
        let mut b = Simulation::from_snapshot(&with_bond);
        for _ in 0..20 {
            a.tick();
            b.tick();
        }
        assert_ne!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn thread_count_does_not_change_hash() {
        // Runs with interactions active — covers both the physics passes and
        // the interaction collect/commit under different thread counts.
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut sim = Simulation::with_rules(&test_config(), test_rules());
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
    fn interactions_conserve_totals() {
        // Transfers subtract and add in separate f32 roundings, so totals are
        // conserved to rounding, not bit-exactly — hence the tolerance.
        let mut sim = Simulation::with_rules(&test_config(), test_rules());
        let totals = |s: &WorldSnapshot| {
            let m: f64 = s.particles.iter().map(|p| p.matter as f64).sum();
            let e: f64 = s.particles.iter().map(|p| p.energy as f64).sum();
            let i: f64 = s.particles.iter().map(|p| p.information as f64).sum();
            (m, e, i)
        };
        let (m0, e0, i0) = totals(&sim.snapshot());
        for _ in 0..300 {
            sim.tick();
        }
        let (m1, e1, i1) = totals(&sim.snapshot());
        assert!(((m1 - m0) / m0).abs() < 1e-4, "matter leaked: {m0} -> {m1}");
        assert!(
            ((e1 - e0) / e0.max(1.0)).abs() < 1e-4,
            "energy leaked: {e0} -> {e1}"
        );
        assert!(
            ((i1 - i0) / i0.max(1.0)).abs() < 1e-4,
            "information leaked: {i0} -> {i1}"
        );
    }

    #[test]
    #[should_panic(expected = "radius")]
    fn oversized_rule_radius_is_rejected() {
        // Rule radius beyond the grid cell would silently miss pairs; the
        // assembly-time validation must refuse it.
        let mut rules = test_rules();
        rules.rules[0].radius = 100.0;
        let _ = Simulation::with_rules(&test_config(), rules);
    }

    #[test]
    fn rules_are_replay_identity() {
        let bare = Simulation::new(&test_config());
        let ruled = Simulation::with_rules(&test_config(), test_rules());
        assert_ne!(
            bare.state_hash(),
            ruled.state_hash(),
            "rule set must be part of replay identity from tick 0"
        );
    }

    #[test]
    fn resume_with_rules_matches_uninterrupted() {
        let mut a = Simulation::with_rules(&test_config(), test_rules());
        let mut b = Simulation::with_rules(&test_config(), test_rules());
        for _ in 0..20 {
            a.tick();
            b.tick();
        }
        let mut resumed = Simulation::from_snapshot(&b.snapshot());
        drop(b);
        for _ in 0..50 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(a.state_hash(), resumed.state_hash());
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
