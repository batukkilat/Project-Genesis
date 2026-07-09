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
pub mod env;
pub mod grid;
pub mod impact;
pub mod interact;
pub mod lod;
pub mod physics;
pub mod snapshot;
pub mod store;

use bevy_ecs::prelude::*;
use genesis_config::{ActionScript, LodPolicy, PhysicsParams, PlayerAction, SimConfig};
use genesis_core::DetRng;

use bonds::BondStore;
use env::EnvFields;
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

/// Adaptive-detail policy resource. Held separately from `Params` (which is
/// `Copy`) because the ladder is a `Vec`. Disabled by default; it becomes part
/// of replay identity when LOD is wired into the snapshot (landing step 5).
#[derive(Resource, Debug, Clone)]
pub struct Lod(pub LodPolicy);

/// Base seed for order-free derived RNG streams (`DetRng::derive`). Drawn
/// once from the master stream at creation; part of replay identity.
#[derive(Resource, Debug, Clone, Copy)]
pub struct StreamSeed(pub u64);

/// Player actions not yet applied (Q-2026-07-08-B), sorted by stamped tick
/// (stable sort — script order wins ties) and drained at the start of each
/// tick. Part of replay identity while pending: an applied action is already
/// state (the env cells it wrote), so only this queue hashes.
#[derive(Resource, Debug, Clone, Default)]
pub struct Pending(pub Vec<PlayerAction>);

fn advance_tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

/// Start-of-tick player-action drain: every action stamped for this tick
/// applies before anything simulates, in queue order. Field edits touch env
/// cells; impacts touch particles (shock + payload) — the pushed payload
/// marks the store dirty, so the following canonicalize rebuilds the layout
/// by full sort.
#[allow(clippy::too_many_arguments)]
fn apply_player_actions(
    mut pending: ResMut<Pending>,
    mut env: ResMut<EnvFields>,
    mut store: ResMut<ParticleStore>,
    mut next_id: ResMut<NextId>,
    geom: Res<GridGeom>,
    stream_seed: Res<StreamSeed>,
    tick: Res<Tick>,
) {
    let due = pending.0.iter().take_while(|a| a.tick == tick.0).count();
    for (k, a) in pending.0.drain(..due).enumerate() {
        match a.action {
            genesis_config::ActionKind::FieldSet { .. }
            | genesis_config::ActionKind::FieldAdd { .. } => env.apply_action(&a.action),
            genesis_config::ActionKind::Impact {
                x,
                y,
                radius,
                impulse,
                energy,
                payload,
            } => impact::apply(
                &mut store,
                &geom,
                x,
                y,
                radius,
                impulse,
                energy,
                &payload,
                stream_seed.0,
                tick.0,
                k as u64,
                &mut next_id.0,
            ),
        }
    }
}

/// Field dynamics (Q-2026-07-08-C): runs after the action drain so a player
/// edit is part of this tick's evolution, and before the particle step so
/// particles see the evolved field. A fully static env skips the pass.
fn step_env_dynamics(mut env: ResMut<EnvFields>, params: Res<Params>) {
    if env.any_dynamic() {
        env.step(params.dt);
    }
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
    lod: Res<Lod>,
    env: Res<EnvFields>,
    stream_seed: Res<StreamSeed>,
    tick: Res<Tick>,
    mut next_id: ResMut<NextId>,
) {
    store.canonicalize(&geom);
    // Classify right after canonicalize so the mask sees the tick's canonical
    // layout and current velocities; the force/interaction/integrate passes
    // then read it. Disabled policy leaves the mask empty (all active).
    lod::classify(&mut store, &geom, &lod.0, tick.0);
    bonds.rebuild(&store);
    physics::forces(&mut store, &geom, &params.physics, &bonds);
    interact::apply(
        &mut store,
        &mut bonds,
        &geom,
        &rules,
        &env,
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

    /// Fresh simulation with rules and no player actions.
    pub fn with_rules(config: &SimConfig, rules: RuleSet) -> Self {
        Self::with_rules_and_actions(config, rules, ActionScript::default())
    }

    /// Fresh simulation: spawns `config.particle_count` particles using RNG
    /// streams derived from `config.seed`. The same validated config, rule
    /// set, and action script always produce the same world.
    pub fn with_rules_and_actions(
        config: &SimConfig,
        rules: RuleSet,
        script: ActionScript,
    ) -> Self {
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
            config.lod.clone(),
            EnvFields::from_spec(&config.env, config.world_width, config.world_height),
            script.actions,
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
            // The policy is now part of the snapshot, so an enabled-LOD run
            // resumes into the identical universe it was saved from.
            snap.lod.clone(),
            EnvFields::from_parts(
                snap.env_cols,
                snap.env_rows,
                snap.env_fields.clone(),
                snap.env_dynamics.clone(),
                snap.world_width,
                snap.world_height,
            ),
            snap.pending_actions.clone(),
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
        lod: LodPolicy,
        env: EnvFields,
        mut actions: Vec<PlayerAction>,
    ) -> Self {
        rules.assert_valid(params.physics.interaction_radius, env.field_count());
        // Stable sort: script order breaks ties within a tick (Q-2026-07-08-B).
        actions.sort_by_key(|a| a.tick);
        for (i, a) in actions.iter().enumerate() {
            assert!(
                a.tick >= tick.0,
                "action {i} is stamped for tick {} but the simulation is at tick {} — \
                 a past-stamped action could never replay identically",
                a.tick,
                tick.0
            );
            let field = match a.action {
                genesis_config::ActionKind::FieldSet { field, .. } => Some(field),
                genesis_config::ActionKind::FieldAdd { field, .. } => Some(field),
                // Impacts touch particles, not env fields — nothing to check.
                genesis_config::ActionKind::Impact { .. } => None,
            };
            if let Some(field) = field {
                assert!(
                    (field as usize) < env.field_count(),
                    "action {i} references env field {field} but the config declares {} field(s)",
                    env.field_count()
                );
            }
        }
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
        world.insert_resource(Lod(lod));
        world.insert_resource(env);
        world.insert_resource(Pending(actions));

        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                apply_player_actions,
                step_env_dynamics,
                sim_step,
                advance_tick,
            )
                .chain(),
        );

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

    /// Number of particles active on the most recent tick's LOD mask, or `None`
    /// when LOD is off (the mask is empty = everything active). Read-only
    /// diagnostic — never affects state; used by the benchmark to report the
    /// active fraction that LOD achieves.
    pub fn active_count(&self) -> Option<usize> {
        let store = self.world.resource::<ParticleStore>();
        if !store.active.is_empty() && store.active.len() == store.len() {
            Some(store.active.iter().filter(|&&a| a).count())
        } else {
            None
        }
    }

    /// Canonical snapshot of the full simulation state, particles sorted by id.
    pub fn snapshot(&self) -> WorldSnapshot {
        let params = *self.world.resource::<Params>();
        let tick = self.world.resource::<Tick>().0;
        let (rng_state, rng_gamma) = self.world.resource::<SimRng>().0.to_parts();
        let next_id = self.world.resource::<NextId>().0;
        let stream_seed = self.world.resource::<StreamSeed>().0;
        let rules = self.world.resource::<RuleSet>().rules.clone();
        let lod = self.world.resource::<Lod>().0.clone();
        let env = self.world.resource::<EnvFields>();
        let pending_actions = self.world.resource::<Pending>().0.clone();
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
            lod,
            env_cols: env.cols,
            env_rows: env.rows,
            env_fields: env.values.clone(),
            env_dynamics: env.dynamics.clone(),
            pending_actions,
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
                env_cond: Vec::new(),
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
            env_cond: Vec::new(),
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
            env_cond: Vec::new(),
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
            env_cond: Vec::new(),
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
            env_cond: Vec::new(),
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
        let mut eat = split.clone();
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

    /// A single-rung, rate-1 ladder: LOD is enabled but every chunk is hot, so
    /// every particle is active every tick. This exercises the whole gated
    /// pipeline (mask built, read by forces/interact/integrate, pushed on emit,
    /// compacted on absorb) with the gate always open.
    fn all_hot_lod() -> LodPolicy {
        LodPolicy {
            enabled: true,
            chunk_cells: 4,
            ladder: vec![genesis_config::LodRung {
                min_activity: 0.0,
                rate: 1,
            }],
        }
    }

    /// A demoting ladder: slow chunks (speed² < 1) run every 8th tick, faster
    /// chunks run every tick. With the default speed range ([0, 2)) plenty of
    /// chunks fall on the cold rung, so LOD actually skips work.
    fn demoting_lod() -> LodPolicy {
        LodPolicy {
            enabled: true,
            chunk_cells: 4,
            ladder: vec![
                genesis_config::LodRung {
                    min_activity: 0.0,
                    rate: 8,
                },
                genesis_config::LodRung {
                    min_activity: 1.0,
                    rate: 1,
                },
            ],
        }
    }

    #[test]
    fn lod_all_hot_produces_identical_state_to_lod_off() {
        // The strongest correctness check on the gate: when the mask marks
        // everything active, every gated pass must produce exactly the particle
        // and bond state it would with LOD off. (The *hash* differs — an
        // enabled policy is a distinct universe by design, even all-hot — so we
        // compare the simulation state directly, not the hash.) Runs the full
        // Phase 3 vocabulary (transfers, bonds, copy, emit/absorb) so all four
        // gates are exercised.
        let off = test_config();
        let mut on = test_config();
        on.lod = all_hot_lod();

        for rules in [test_rules(), bonding_rules(), fission_rules()] {
            let mut a = Simulation::with_rules(&off, rules.clone());
            let mut b = Simulation::with_rules(&on, rules);
            for _ in 0..150 {
                a.tick();
                b.tick();
            }
            let (sa, sb) = (a.snapshot(), b.snapshot());
            assert_eq!(
                sa.particles, sb.particles,
                "all-hot LOD diverged from LOD-off — the gate corrupts particle state"
            );
            assert_eq!(
                sa.bonds, sb.bonds,
                "all-hot LOD diverged from LOD-off — the gate corrupts bonds"
            );
        }
    }

    #[test]
    fn disabled_lod_is_not_replay_identity() {
        // A disabled policy has no effect, so it must not perturb the hash:
        // two disabled policies that differ only cosmetically (chunk_cells)
        // produce byte-identical simulations AND identical hashes — a LOD-off
        // run keeps the exact identity it had before LOD existed.
        let mut a = test_config();
        a.lod = LodPolicy {
            enabled: false,
            chunk_cells: 4,
            ..LodPolicy::default()
        };
        let mut b = test_config();
        b.lod = LodPolicy {
            enabled: false,
            chunk_cells: 9,
            ladder: vec![
                genesis_config::LodRung {
                    min_activity: 0.0,
                    rate: 16,
                },
                genesis_config::LodRung {
                    min_activity: 2.0,
                    rate: 1,
                },
            ],
        };
        let mut sa = Simulation::with_rules(&a, bonding_rules());
        let mut sb = Simulation::with_rules(&b, bonding_rules());
        assert_eq!(sa.state_hash(), sb.state_hash());
        for _ in 0..80 {
            sa.tick();
            sb.tick();
        }
        assert_eq!(
            sa.state_hash(),
            sb.state_hash(),
            "a disabled policy perturbed replay identity"
        );
    }

    #[test]
    fn enabled_lod_policy_is_replay_identity() {
        // Two enabled policies that differ produce different universes: even
        // all-hot (which yields identical particle state) hashes distinctly
        // from a demoting ladder, and both differ from LOD-off.
        let off = Simulation::with_rules(&test_config(), bonding_rules()).state_hash();

        let mut hot = test_config();
        hot.lod = all_hot_lod();
        let hot_hash = Simulation::with_rules(&hot, bonding_rules()).state_hash();

        let mut demote = test_config();
        demote.lod = demoting_lod();
        let demote_hash = Simulation::with_rules(&demote, bonding_rules()).state_hash();

        assert_ne!(off, hot_hash, "enabled policy must change replay identity");
        assert_ne!(off, demote_hash);
        assert_ne!(
            hot_hash, demote_hash,
            "different policy = different universe"
        );
    }

    #[test]
    fn resume_with_enabled_lod_matches_uninterrupted() {
        // The policy is in the snapshot (v8), so an enabled-LOD run resumes
        // into the identical universe it was saved from.
        let mut on = test_config();
        on.lod = demoting_lod();
        let mut a = Simulation::with_rules(&on, bonding_rules());
        let mut b = Simulation::with_rules(&on, bonding_rules());
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
    fn lod_conserves_matter_and_energy_across_rate_boundaries() {
        // The hard LOD invariant (Q-2026-07-06-A, item 2): matter and energy
        // *stocks* are conserved exactly at every rate, including across chunks
        // straddling different rates. An interaction event fires only between
        // two active particles and each event already conserves, so a frozen
        // particle never half-participates — nothing to leak. Exercised with
        // an emit/absorb pack (fission_rules) under a demoting ladder, so the
        // world genuinely has hot and cold chunks trading particles.
        let mut on = test_config();
        on.lod = demoting_lod();
        let mut sim = Simulation::with_rules(&on, fission_rules());
        let totals = |s: &WorldSnapshot| {
            let m: f64 = s.particles.iter().map(|p| p.matter as f64).sum();
            let e: f64 = s.particles.iter().map(|p| p.energy as f64).sum();
            (m, e)
        };
        let (m0, e0) = totals(&sim.snapshot());
        for _ in 0..200 {
            sim.tick();
        }
        let snap = sim.snapshot();
        // The population must actually churn (emit/absorb fired), or the test
        // is vacuous.
        assert_ne!(snap.particles.len(), 500, "no create/destroy happened");
        let (m1, e1) = totals(&snap);
        assert!(
            ((m1 - m0) / m0).abs() < 1e-4,
            "matter leaked under cross-rate LOD: {m0} -> {m1}"
        );
        assert!(
            ((e1 - e0) / e0.max(1.0)).abs() < 1e-4,
            "energy leaked under cross-rate LOD: {e0} -> {e1}"
        );
    }

    #[test]
    fn lod_enabled_changes_the_future() {
        // A demoting ladder freezes quiet chunks, so trajectories differ from a
        // LOD-off run. (LOD-on and LOD-off are different universes by design.)
        let off = test_config();
        let mut on = test_config();
        on.lod = demoting_lod();
        let mut a = Simulation::new(&off);
        let mut b = Simulation::new(&on);
        for _ in 0..80 {
            a.tick();
            b.tick();
        }
        assert_ne!(
            a.state_hash(),
            b.state_hash(),
            "demoting LOD changed nothing — the cold rung never froze anyone"
        );
    }

    #[test]
    fn lod_enabled_is_deterministic_and_thread_invariant() {
        // Same seed + config + policy = same simulation, and the mask is an
        // order-independent reduction, so thread count cannot change a bit.
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut on = test_config();
                on.lod = demoting_lod();
                let mut sim = Simulation::with_rules(&on, bonding_rules());
                for _ in 0..120 {
                    sim.tick();
                }
                sim.state_hash()
            })
        };
        assert_eq!(run(1), run(1), "LOD run not deterministic");
        assert_eq!(run(1), run(4), "LOD broke thread-count invariance");
    }

    #[test]
    fn lod_freezes_inactive_particles_bit_for_bit() {
        // Directly observe the freeze: a chunk on the cold (rate-8) rung must
        // leave its particles' state untouched on an off-stride tick.
        let mut on = test_config();
        on.lod = demoting_lod();
        // Very slow start so most chunks land on the cold rung.
        on.initial.speed = genesis_config::Range::new(0.0, 0.2);
        let mut sim = Simulation::new(&on);
        // Advance to a tick where the cold rung (rate 8) is off-stride.
        sim.tick(); // now at tick 1
        let before = sim.snapshot();
        // Tick 2, 3 are also off-stride for rate 8 (and rate 1 chunks move).
        sim.tick();
        let after = sim.snapshot();
        let frozen = before
            .particles
            .iter()
            .zip(&after.particles)
            .filter(|(a, b)| {
                a.id == b.id
                    && a.pos_x == b.pos_x
                    && a.pos_y == b.pos_y
                    && a.vel_x == b.vel_x
                    && a.vel_y == b.vel_y
            })
            .count();
        assert!(
            frozen > 0,
            "no particle was frozen — the cold rung never engaged"
        );
    }

    #[test]
    fn information_max_caps_the_full_sim_and_is_replay_identity() {
        // End-to-end (config -> params -> sim_step -> apply), not the direct
        // apply() unit tests: the amplifying info_copy in bonding_rules would
        // ratchet information upward unbounded; a low information_max clamps
        // it. The cap sits just above the initial ceiling (info starts in
        // [0, 1)), so a single amplifying copy already crosses it.
        let cap = 1.2f32;
        let mut capped = test_config();
        capped.physics.information_max = cap;
        let mut uncapped = test_config();
        uncapped.physics.information_max = 1e30;

        let mut sim_capped = Simulation::with_rules(&capped, bonding_rules());
        let mut sim_uncapped = Simulation::with_rules(&uncapped, bonding_rules());
        for _ in 0..300 {
            sim_capped.tick();
            sim_uncapped.tick();
        }

        // Under the cap every particle stays at or below it, and finite.
        for p in &sim_capped.snapshot().particles {
            assert!(
                p.information.is_finite(),
                "information went non-finite under the cap"
            );
            assert!(
                p.information <= cap,
                "information {} exceeded the cap {cap}",
                p.information
            );
        }

        // The cap is part of replay identity: differing only in
        // information_max, the two runs diverge — which also proves the cap
        // actually engaged (amplification crossed it).
        assert_ne!(
            sim_capped.state_hash(),
            sim_uncapped.state_hash(),
            "information_max changed nothing — cap never engaged, or it is not \
             in replay identity"
        );

        // And it survives save/resume bit-for-bit.
        let mut resumed = Simulation::from_snapshot(&sim_capped.snapshot());
        for _ in 0..40 {
            sim_capped.tick();
            resumed.tick();
        }
        assert_eq!(sim_capped.state_hash(), resumed.state_hash());
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

    fn env_with(fields: Vec<genesis_config::EnvFieldSpec>) -> genesis_config::EnvSpec {
        genesis_config::EnvSpec {
            cols: 8,
            rows: 8,
            fields,
        }
    }

    fn field(init: genesis_config::FieldInit) -> genesis_config::EnvFieldSpec {
        genesis_config::EnvFieldSpec {
            name: String::new(),
            init,
            dynamics: Default::default(),
        }
    }

    #[test]
    fn empty_env_is_not_replay_identity() {
        // No declared fields = no environment: grid dims are never read, so
        // two configs differing only in empty-env cosmetics produce identical
        // universes AND identical hashes — pre-env runs keep their identity.
        let mut a = test_config();
        a.env = genesis_config::EnvSpec {
            cols: 4,
            rows: 4,
            fields: vec![],
        };
        let mut b = test_config();
        b.env = genesis_config::EnvSpec {
            cols: 64,
            rows: 16,
            fields: vec![],
        };
        let mut sa = Simulation::with_rules(&a, bonding_rules());
        let mut sb = Simulation::with_rules(&b, bonding_rules());
        assert_eq!(sa.state_hash(), sb.state_hash());
        for _ in 0..50 {
            sa.tick();
            sb.tick();
        }
        assert_eq!(
            sa.state_hash(),
            sb.state_hash(),
            "an empty env spec perturbed replay identity"
        );
    }

    #[test]
    fn declared_env_fields_are_replay_identity() {
        use genesis_config::FieldInit;
        let bare = Simulation::new(&test_config()).state_hash();

        let mut uniform = test_config();
        uniform.env = env_with(vec![field(FieldInit::Uniform(1.0))]);
        let uniform_hash = Simulation::new(&uniform).state_hash();

        let mut other_value = test_config();
        other_value.env = env_with(vec![field(FieldInit::Uniform(2.0))]);
        let other_hash = Simulation::new(&other_value).state_hash();

        assert_ne!(bare, uniform_hash, "declared fields must change identity");
        assert_ne!(uniform_hash, other_hash, "cell values are the identity");

        // Identity is the *values*, not the init spec: a gradient that
        // degenerates to the same constant is the same universe.
        let mut flat_gradient = test_config();
        flat_gradient.env = env_with(vec![field(FieldInit::GradientX { lo: 1.0, hi: 1.0 })]);
        assert_eq!(
            uniform_hash,
            Simulation::new(&flat_gradient).state_hash(),
            "init specs that produce identical cell values must hash alike"
        );
    }

    fn set_action(tick: u64, x0: f32, x1: f32, value: f32) -> genesis_config::PlayerAction {
        genesis_config::PlayerAction {
            tick,
            action: genesis_config::ActionKind::FieldSet {
                field: 0,
                region: genesis_config::RegionSpec {
                    x0,
                    y0: 0.0,
                    x1,
                    y1: 256.0,
                },
                value,
            },
        }
    }

    fn script(actions: Vec<genesis_config::PlayerAction>) -> ActionScript {
        ActionScript { actions }
    }

    #[test]
    fn actions_apply_at_their_stamped_tick() {
        // Field starts uniform 0; an action at tick 10 sets the west half to
        // 2. The edit must be absent at tick 10's start-of-tick snapshot
        // boundary (we observe after tick 9) and present after tick 10.
        let mut config = test_config();
        config.env = env_with(vec![field(genesis_config::FieldInit::Uniform(0.0))]);
        let mut sim = Simulation::with_rules_and_actions(
            &config,
            RuleSet::default(),
            script(vec![set_action(10, 0.0, 128.0, 2.0)]),
        );
        for _ in 0..10 {
            sim.tick();
        }
        // Ticks 0..9 have run; the tick-10 drain has not happened yet.
        let before = sim.snapshot();
        assert!(
            before.env_fields[0].iter().all(|&v| v == 0.0),
            "edit applied too early"
        );
        assert_eq!(before.pending_actions.len(), 1, "action must still pend");
        sim.tick();
        let after = sim.snapshot();
        assert!(after.pending_actions.is_empty(), "action must be consumed");
        // 8x8 env grid over 256: west-half columns 0..3 edited, east half not.
        for cy in 0..8 {
            for cx in 0..8 {
                let v = after.env_fields[0][cy * 8 + cx];
                if cx < 4 {
                    assert_eq!(v, 2.0, "west cell ({cx}, {cy}) not edited");
                } else {
                    assert_eq!(v, 0.0, "east cell ({cx}, {cy}) wrongly edited");
                }
            }
        }
    }

    #[test]
    fn scripted_run_is_deterministic_and_thread_invariant() {
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut config = gradient_config();
                config.env = env_with(vec![field(genesis_config::FieldInit::Uniform(0.0))]);
                let mut sim = Simulation::with_rules_and_actions(
                    &config,
                    banded_bond_rules(),
                    script(vec![
                        set_action(20, 128.0, 256.0, 1.0),
                        set_action(60, 0.0, 128.0, 0.6),
                    ]),
                );
                for _ in 0..100 {
                    sim.tick();
                }
                sim.state_hash()
            })
        };
        assert_eq!(run(1), run(1), "scripted run not deterministic");
        assert_eq!(run(1), run(4), "actions broke thread-count invariance");
    }

    #[test]
    fn pending_actions_are_replay_identity_applied_ones_are_state() {
        // Pending: a queued future edit is a different universe from tick 0.
        let mut config = test_config();
        config.env = env_with(vec![field(genesis_config::FieldInit::Uniform(1.0))]);
        let bare = Simulation::with_rules(&config, RuleSet::default());
        let queued = Simulation::with_rules_and_actions(
            &config,
            RuleSet::default(),
            script(vec![set_action(1000, 0.0, 128.0, 5.0)]),
        );
        assert_ne!(
            bare.state_hash(),
            queued.state_hash(),
            "a pending action must be replay identity"
        );

        // Applied: an action that rewrites the value already there leaves a
        // byte-identical simulation once consumed — identical hash, exactly
        // like the disabled-LOD / empty-env precedents.
        let mut noop_sim = Simulation::with_rules_and_actions(
            &config,
            RuleSet::default(),
            script(vec![set_action(3, 0.0, 128.0, 1.0)]),
        );
        let mut bare_sim = Simulation::with_rules(&config, RuleSet::default());
        for _ in 0..10 {
            noop_sim.tick();
            bare_sim.tick();
        }
        assert_eq!(
            noop_sim.state_hash(),
            bare_sim.state_hash(),
            "an applied no-op edit left a trace — applied actions must be \
             state only, never separately hashed"
        );
    }

    #[test]
    fn resume_mid_script_matches_uninterrupted() {
        let mut config = gradient_config();
        config.env = env_with(vec![field(genesis_config::FieldInit::Uniform(0.0))]);
        let make = || {
            Simulation::with_rules_and_actions(
                &config,
                banded_bond_rules(),
                script(vec![
                    set_action(20, 128.0, 256.0, 1.0),
                    set_action(60, 0.0, 128.0, 1.0),
                ]),
            )
        };
        let mut a = make();
        let mut b = make();
        for _ in 0..40 {
            a.tick();
            b.tick();
        }
        let snap = b.snapshot();
        assert_eq!(
            snap.pending_actions.len(),
            1,
            "the tick-60 action must still pend in the save"
        );
        let mut resumed = Simulation::from_snapshot(&snap);
        drop(b);
        for _ in 0..40 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(
            a.state_hash(),
            resumed.state_hash(),
            "resume mid-script diverged from the uninterrupted run"
        );
    }

    #[test]
    fn scripted_environment_change_redirects_emergence() {
        // The Phase 4 exit criterion end-to-end: a player script shapes the
        // environment, and *where* structure emerges follows. The bond gate
        // needs field >= 0.5 but the world starts uniform 0 — nothing can
        // bond anywhere. At tick 50 the script opens the east half.
        let mut config = gradient_config();
        config.env = env_with(vec![field(genesis_config::FieldInit::Uniform(0.0))]);
        let mut sim = Simulation::with_rules_and_actions(
            &config,
            banded_bond_rules(),
            script(vec![set_action(50, 128.0, 256.0, 1.0)]),
        );
        for _ in 0..50 {
            sim.tick();
        }
        assert!(
            sim.snapshot().bonds.is_empty(),
            "no bonds may form before the script opens the gate"
        );
        for _ in 0..100 {
            sim.tick();
        }
        let snap = sim.snapshot();
        assert!(
            !snap.bonds.is_empty(),
            "the scripted edit opened the gate — bonds must have formed"
        );
        let x_of: std::collections::BTreeMap<u64, f32> =
            snap.particles.iter().map(|p| (p.id, p.pos_x)).collect();
        let bonded_x: Vec<f32> = snap
            .bonds
            .iter()
            .flat_map(|b| [x_of[&b.a], x_of[&b.b]])
            .collect();
        let west = bonded_x.iter().filter(|&&x| x < 100.0).count();
        assert!(
            (west as f32) < 0.05 * bonded_x.len() as f32,
            "{west} of {} bonded endpoints in the still-closed west half",
            bonded_x.len()
        );
    }

    fn impact_action(tick: u64, x: f32, y: f32, count: u32) -> genesis_config::PlayerAction {
        genesis_config::PlayerAction {
            tick,
            action: genesis_config::ActionKind::Impact {
                x,
                y,
                radius: 24.0,
                impulse: 2.0,
                energy: 5.0,
                payload: genesis_config::PayloadSpec {
                    count,
                    matter: genesis_config::Range::new(0.3, 0.9),
                    energy: genesis_config::Range::new(0.5, 1.5),
                    information: genesis_config::Range::new(0.0, 0.2),
                    speed: genesis_config::Range::new(0.2, 1.0),
                    spread: 6.0,
                },
            },
        }
    }

    #[test]
    fn impact_injects_exactly_its_payload_plus_shock() {
        // The one deliberate exception to closed-world conservation
        // (decisions log 2026-07-06: external material): an impact injects
        // matter/energy, and the injection is *bounded exactly* by what the
        // action declares. Twin runs with and without the impact, no rules —
        // physics conserves matter by construction and never touches the
        // energy quantity, so the difference is the impact alone.
        let totals = |s: &WorldSnapshot| {
            let m: f64 = s.particles.iter().map(|p| p.matter as f64).sum();
            let e: f64 = s.particles.iter().map(|p| p.energy as f64).sum();
            (m, e)
        };
        let count = 40u32;
        let mut with = Simulation::with_rules_and_actions(
            &test_config(),
            RuleSet::default(),
            script(vec![impact_action(10, 128.0, 128.0, count)]),
        );
        let mut without = Simulation::new(&test_config());
        for _ in 0..20 {
            with.tick();
            without.tick();
        }
        let (snap_with, snap_without) = (with.snapshot(), without.snapshot());
        assert_eq!(
            snap_with.particles.len(),
            snap_without.particles.len() + count as usize,
            "payload count must arrive exactly"
        );
        assert_eq!(snap_with.next_id, snap_without.next_id + count as u64);
        let (m1, e1) = totals(&snap_with);
        let (m0, e0) = totals(&snap_without);
        // Payload quantities are RNG draws inside declared ranges; the shock
        // energy (5.0) deposits in full — the world is dense enough that the
        // 24-radius shock at the world center always finds particles.
        let (dm, de) = (m1 - m0, e1 - e0);
        let n = count as f64;
        assert!(
            dm >= n * 0.3 - 1e-3 && dm <= n * 0.9 + 1e-3,
            "matter injection {dm} outside declared payload range"
        );
        assert!(
            de >= n * 0.5 + 5.0 - 1e-2 && de <= n * 1.5 + 5.0 + 1e-2,
            "energy injection {de} outside payload range + shock"
        );
    }

    #[test]
    fn scripted_impact_is_deterministic_and_thread_invariant() {
        // Impacts run through the full stack (drain -> shock -> payload RNG
        // -> canonicalize -> physics + rules): same seed + script must be one
        // universe on any thread count.
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut sim = Simulation::with_rules_and_actions(
                    &test_config(),
                    bonding_rules(),
                    script(vec![
                        impact_action(20, 64.0, 64.0, 30),
                        impact_action(20, 192.0, 192.0, 30), // same-tick sibling
                        impact_action(60, 128.0, 128.0, 0),  // pure shock
                    ]),
                );
                for _ in 0..100 {
                    sim.tick();
                }
                sim.state_hash()
            })
        };
        assert_eq!(run(1), run(1), "scripted impacts not deterministic");
        assert_eq!(run(1), run(4), "impacts broke thread-count invariance");
    }

    #[test]
    fn resume_with_pending_impact_matches_uninterrupted() {
        let make = || {
            Simulation::with_rules_and_actions(
                &test_config(),
                bonding_rules(),
                script(vec![impact_action(60, 128.0, 128.0, 25)]),
            )
        };
        let mut a = make();
        let mut b = make();
        for _ in 0..40 {
            a.tick();
            b.tick();
        }
        let snap = b.snapshot();
        assert_eq!(snap.pending_actions.len(), 1, "impact must still pend");
        let mut resumed = Simulation::from_snapshot(&snap);
        drop(b);
        for _ in 0..40 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(
            a.state_hash(),
            resumed.state_hash(),
            "resume diverged — the pending impact (payload RNG included) \
             must fire identically after restore"
        );
    }

    #[test]
    fn impact_on_the_save_tick_resumes_bit_identically() {
        // Regression (2026-07-09 night review): a save taken at the impact's
        // own tick drains it on the snapshot's id-sorted layout, while the
        // uninterrupted run drains on the previous tick's canonical layout.
        // The energy deposit normalizes by an f32 weight sum, so a
        // layout-order-dependent sum diverges bitwise; impact::apply must
        // sum in id order. The wide radius guarantees many in-radius
        // particles with distinct weights.
        let wide_impact = |tick: u64| {
            let mut a = impact_action(tick, 128.0, 128.0, 25);
            if let genesis_config::ActionKind::Impact { radius, energy, .. } = &mut a.action {
                *radius = 100.0;
                *energy = 40.0;
            }
            a
        };
        let make = || {
            Simulation::with_rules_and_actions(
                &test_config(),
                bonding_rules(),
                script(vec![wide_impact(40)]),
            )
        };
        let mut a = make();
        let mut b = make();
        for _ in 0..40 {
            a.tick();
            b.tick();
        }
        let snap = b.snapshot();
        assert_eq!(
            snap.pending_actions.len(),
            1,
            "the impact fires at the start of tick 40's step, so a save at \
             tick-count 40 must still carry it"
        );
        let mut resumed = Simulation::from_snapshot(&snap);
        drop(b);
        for _ in 0..20 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(
            a.state_hash(),
            resumed.state_hash(),
            "impact stamped for the save tick diverged on resume — the shock \
             weight sum must not depend on store layout order"
        );
    }

    #[test]
    fn pending_impact_is_replay_identity() {
        // Two runs with identical state but different queued impacts have
        // different futures, so they must hash apart from tick 0 — and from
        // an action-free run.
        let bare = Simulation::new(&test_config()).state_hash();
        let queued = Simulation::with_rules_and_actions(
            &test_config(),
            RuleSet::default(),
            script(vec![impact_action(1000, 64.0, 64.0, 10)]),
        )
        .state_hash();
        let queued_other = Simulation::with_rules_and_actions(
            &test_config(),
            RuleSet::default(),
            script(vec![impact_action(1000, 64.0, 64.0, 11)]),
        )
        .state_hash();
        assert_ne!(bare, queued, "a pending impact must be replay identity");
        assert_ne!(queued, queued_other, "every impact parameter is identity");
    }

    #[test]
    #[should_panic(expected = "past-stamped")]
    fn past_stamped_action_is_rejected() {
        // Resume at tick 40 with a hand-built snapshot carrying a tick-10
        // action: it could never replay identically, so assembly must refuse.
        let mut config = test_config();
        config.env = env_with(vec![field(genesis_config::FieldInit::Uniform(0.0))]);
        let mut sim = Simulation::with_rules_and_actions(
            &config,
            RuleSet::default(),
            script(vec![set_action(100, 0.0, 128.0, 1.0)]),
        );
        for _ in 0..40 {
            sim.tick();
        }
        let mut snap = sim.snapshot();
        snap.pending_actions[0].tick = 10;
        let _ = Simulation::from_snapshot(&snap);
    }

    #[test]
    #[should_panic(expected = "references env field")]
    fn action_referencing_missing_env_field_is_rejected() {
        // No env fields declared, but the script edits field 0.
        let _ = Simulation::with_rules_and_actions(
            &test_config(),
            RuleSet::default(),
            script(vec![set_action(10, 0.0, 128.0, 1.0)]),
        );
    }

    #[test]
    fn field_dynamics_are_replay_identity_only_when_active() {
        use genesis_config::{FieldDynamics, FieldInit};
        // Static dynamics contribute nothing: explicit zeros hash like an
        // omitted dynamics block.
        let mut static_cfg = test_config();
        static_cfg.env = env_with(vec![field(FieldInit::Uniform(1.0))]);
        let mut explicit_zero = static_cfg.clone();
        explicit_zero.env.fields[0].dynamics = FieldDynamics::default();
        assert_eq!(
            Simulation::new(&static_cfg).state_hash(),
            Simulation::new(&explicit_zero).state_hash()
        );

        // An active rate is a different universe from tick 0, before any cell
        // value has changed.
        let mut dynamic_cfg = static_cfg.clone();
        dynamic_cfg.env.fields[0].dynamics = FieldDynamics {
            diffusion: 1.0,
            relax_rate: 0.0,
            relax_to: 0.0,
        };
        assert_ne!(
            Simulation::new(&static_cfg).state_hash(),
            Simulation::new(&dynamic_cfg).state_hash(),
            "active dynamics must enter replay identity before they act"
        );
    }

    #[test]
    fn field_dynamics_evolve_and_survive_save_resume() {
        use genesis_config::{FieldDynamics, FieldInit};
        // A gradient field relaxing toward 0.5 while diffusing: the field
        // must actually change over ticks, and a mid-run save must resume
        // into the identical future (the params live in the save).
        let mut config = test_config();
        config.env = env_with(vec![field(FieldInit::GradientX { lo: 0.0, hi: 1.0 })]);
        config.env.fields[0].dynamics = FieldDynamics {
            diffusion: 2.0,
            relax_rate: 0.2,
            relax_to: 0.5,
        };
        let mut a = Simulation::new(&config);
        let mut b = Simulation::new(&config);
        let start = a.snapshot().env_fields[0].clone();
        for _ in 0..40 {
            a.tick();
            b.tick();
        }
        let snap = b.snapshot();
        assert_ne!(
            snap.env_fields[0], start,
            "dynamic field never evolved over 40 ticks"
        );
        assert_eq!(snap.env_dynamics[0].diffusion, 2.0);
        let mut resumed = Simulation::from_snapshot(&snap);
        drop(b);
        for _ in 0..40 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(
            a.state_hash(),
            resumed.state_hash(),
            "resume diverged — dynamics params lost across save/load"
        );
    }

    #[test]
    fn env_fields_survive_save_resume() {
        use genesis_config::FieldInit;
        let mut config = test_config();
        config.env = env_with(vec![
            field(FieldInit::GradientX { lo: 0.0, hi: 4.0 }),
            field(FieldInit::Uniform(0.5)),
        ]);
        let mut a = Simulation::with_rules(&config, bonding_rules());
        let mut b = Simulation::with_rules(&config, bonding_rules());
        for _ in 0..30 {
            a.tick();
            b.tick();
        }
        let snap = b.snapshot();
        assert_eq!(snap.env_fields.len(), 2);
        let mut resumed = Simulation::from_snapshot(&snap);
        drop(b);
        assert_eq!(a.state_hash(), resumed.state_hash());
        for _ in 0..30 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(a.state_hash(), resumed.state_hash());
    }

    /// A bond-forming rule gated to the east half of a west→east gradient
    /// (field 0 in [0.5, ∞) over a 0→1 ramp; with 8 env cols over a 256-wide
    /// world, the gate opens at x = 128).
    fn banded_bond_rules() -> RuleSet {
        let mut rule = test_rules().rules[0].clone();
        rule.radius = 4.0;
        rule.probability = 0.2;
        rule.transfer_matter = 0.0;
        rule.transfer_energy = 0.0;
        rule.transfer_information = 0.0;
        rule.bond_action = interact::BondAction::Create;
        rule.bond_strength = 3.0;
        rule.env_cond = vec![interact::EnvBound {
            field: 0,
            bounds: interact::Bounds {
                min: 0.5,
                max: f32::INFINITY,
            },
        }];
        RuleSet { rules: vec![rule] }
    }

    fn gradient_config() -> SimConfig {
        let mut config = test_config();
        // Slow start: bonds should stay near where the environment allowed
        // them to form, so the shaping test below reads formation location,
        // not later drift.
        config.initial.speed = genesis_config::Range::new(0.0, 0.5);
        config.env = env_with(vec![field(genesis_config::FieldInit::GradientX {
            lo: 0.0,
            hi: 1.0,
        })]);
        config
    }

    #[test]
    fn env_gradient_shapes_where_structures_emerge() {
        // The Phase 4 exit criterion in miniature: identical physics and
        // rules everywhere, but bonding is env-gated to the east half. The
        // *location* of emergent structure must follow the environment —
        // nothing about position is authored in the rule itself.
        let mut sim = Simulation::with_rules(&gradient_config(), banded_bond_rules());
        for _ in 0..100 {
            sim.tick();
        }
        let snap = sim.snapshot();
        assert!(
            !snap.bonds.is_empty(),
            "the in-band bond rule never fired — test is vacuous"
        );
        let x_of: std::collections::BTreeMap<u64, f32> =
            snap.particles.iter().map(|p| (p.id, p.pos_x)).collect();
        let bonded_x: Vec<f32> = snap
            .bonds
            .iter()
            .flat_map(|b| [x_of[&b.a], x_of[&b.b]])
            .collect();
        // The gate opens at x = 128; endpoints can sit one rule-radius west
        // of an in-band initiator and drift somewhat after bonding, so allow
        // a small straggler tail rather than a hard boundary.
        let west = bonded_x.iter().filter(|&&x| x < 100.0).count();
        assert!(
            (west as f32) < 0.05 * bonded_x.len() as f32,
            "{west} of {} bonded endpoints deep in the gated-off west half",
            bonded_x.len()
        );
        let mean_x: f32 = bonded_x.iter().sum::<f32>() / bonded_x.len() as f32;
        assert!(
            mean_x > 140.0,
            "bonded structure should concentrate in the east half, mean x = {mean_x}"
        );
    }

    #[test]
    fn env_gate_is_replay_identity() {
        // Same config, same rules except one env gate: different universe.
        let gated = Simulation::with_rules(&gradient_config(), banded_bond_rules());
        let mut ungated_rules = banded_bond_rules();
        ungated_rules.rules[0].env_cond.clear();
        let ungated = Simulation::with_rules(&gradient_config(), ungated_rules);
        assert_ne!(
            gated.state_hash(),
            ungated.state_hash(),
            "an env gate must be part of replay identity from tick 0"
        );
    }

    #[test]
    fn env_gated_rules_are_deterministic_and_thread_invariant() {
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut sim = Simulation::with_rules(&gradient_config(), banded_bond_rules());
                for _ in 0..80 {
                    sim.tick();
                }
                sim.state_hash()
            })
        };
        assert_eq!(run(1), run(1), "env-gated run not deterministic");
        assert_eq!(run(1), run(4), "env gating broke thread-count invariance");
    }

    #[test]
    fn resume_with_env_gated_rules_matches_uninterrupted() {
        let mut a = Simulation::with_rules(&gradient_config(), banded_bond_rules());
        let mut b = Simulation::with_rules(&gradient_config(), banded_bond_rules());
        for _ in 0..60 {
            a.tick();
            b.tick();
        }
        let snap = b.snapshot();
        assert!(!snap.bonds.is_empty(), "test needs live bonds at the save");
        assert!(!snap.rules[0].env_cond.is_empty(), "gate lost by snapshot");
        let mut resumed = Simulation::from_snapshot(&snap);
        drop(b);
        for _ in 0..60 {
            a.tick();
            resumed.tick();
        }
        assert_eq!(a.state_hash(), resumed.state_hash());
    }

    #[test]
    #[should_panic(expected = "env_cond references field")]
    fn rule_referencing_missing_env_field_is_rejected() {
        // A gate on field 0 with no declared fields must fail at assembly,
        // not panic mid-tick in the hot loop.
        let _ = Simulation::with_rules(&test_config(), banded_bond_rules());
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
