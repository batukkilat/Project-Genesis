//! The Observer's constitutional guarantee, as a test: running with the
//! Observer on or off produces identical simulation state hashes
//! (Prompts/spec/Observer.md "Replay compatible"). The crate never receives
//! a mutable reference to simulation state, so this can only fail if the
//! observed run somehow *reads differently* — the test pins the guarantee
//! against future refactors all the same.

use genesis_config::{Range, SimConfig};
use genesis_observer::{ObserverConfig, StructureTracker};
use genesis_sim::Simulation;
use genesis_sim::interact::{BondAction, Bounds, CompiledRule, QuantityCondition, RuleSet};

fn config() -> SimConfig {
    let mut config = SimConfig {
        seed: 11,
        particle_count: 400,
        world_width: 256.0,
        world_height: 256.0,
        ..Default::default()
    };
    config.initial.information = Range::new(0.0, 1.0);
    config
}

fn bonding_rules() -> RuleSet {
    RuleSet {
        rules: vec![CompiledRule {
            radius: 4.0,
            env_cond: Vec::new(),
            self_cond: QuantityCondition::ANY,
            other_cond: QuantityCondition {
                matter: Bounds::ANY,
                energy: Bounds {
                    min: 0.2,
                    max: f32::INFINITY,
                },
                information: Bounds::ANY,
            },
            probability: 0.2,
            transfer_matter: 0.0,
            transfer_energy: 0.01,
            transfer_information: 0.0,
            bond_action: BondAction::Create,
            bond_strength: 2.0,
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

#[test]
fn observer_on_or_off_identical_state_hashes() {
    // Unobserved run.
    let mut bare = Simulation::with_rules(&config(), bonding_rules());
    for _ in 0..120 {
        bare.tick();
    }

    // Observed run: sample every 10 ticks, full observer pipeline.
    let mut watched = Simulation::with_rules(&config(), bonding_rules());
    let mut tracker = StructureTracker::new(ObserverConfig {
        persist_after: 3,
        ..ObserverConfig::default()
    });
    let mut samples = 0;
    for i in 1..=120u32 {
        watched.tick();
        if i % 10 == 0 {
            let snap = watched.snapshot();
            let comps = genesis_observer::bond_components(&snap);
            let _stats = genesis_observer::sample_stats(&snap, &comps);
            let _report = tracker.observe(&comps);
            samples += 1;
        }
    }
    assert_eq!(samples, 12);

    assert_eq!(
        bare.state_hash(),
        watched.state_hash(),
        "observing the simulation changed it — the Observer must be read-only"
    );
}

#[test]
fn observer_output_is_deterministic_for_the_same_run() {
    let observe = || {
        let mut sim = Simulation::with_rules(&config(), bonding_rules());
        let mut tracker = StructureTracker::new(ObserverConfig {
            persist_after: 3,
            ..ObserverConfig::default()
        });
        let mut trace = Vec::new();
        for i in 1..=100u32 {
            sim.tick();
            if i % 10 == 0 {
                let snap = sim.snapshot();
                let comps = genesis_observer::bond_components(&snap);
                trace.push((comps.clone(), tracker.observe(&comps)));
            }
        }
        trace
    };
    assert_eq!(
        observe(),
        observe(),
        "same run, same cadence — the observer must report identically"
    );
}
