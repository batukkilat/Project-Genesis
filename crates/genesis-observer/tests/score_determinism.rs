//! Run scoring is deterministic (Phase 6.5 deliverable 1): scoring the same
//! run twice — fresh simulations, same seed, same cadence — must produce the
//! identical RunScore, bit for bit. The score is a pure function of the
//! timeline, and the timeline is a pure function of the run, so any
//! divergence here means observer aggregation smuggled in ordering or
//! platform dependence.

use genesis_config::{Range, SimConfig};
use genesis_observer::{ObserverConfig, RunScore, StructureTracker, Timeline};
use genesis_sim::Simulation;
use genesis_sim::interact::{BondAction, Bounds, CompiledRule, QuantityCondition, RuleSet};

fn config() -> SimConfig {
    let mut config = SimConfig {
        seed: 23,
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

/// Run the full pipeline — sim, sampling, timeline, score — from scratch.
fn score_run() -> (RunScore, u64) {
    let observer_config = ObserverConfig {
        persist_after: 3,
        ..ObserverConfig::default()
    };
    let mut sim = Simulation::with_rules(&config(), bonding_rules());
    let mut tracker = StructureTracker::new(observer_config);
    let mut timeline = Timeline::new(observer_config);
    for i in 1..=200u32 {
        sim.tick();
        if i % 10 == 0 {
            let snap = sim.snapshot();
            let comps = genesis_observer::bond_components(&snap);
            let stats = genesis_observer::sample_stats(&snap, &comps);
            tracker.observe(&comps);
            let metrics = genesis_observer::structure_metrics(&snap, &tracker);
            timeline.record(stats, metrics);
        }
    }
    (RunScore::from_timeline(&timeline), sim.state_hash())
}

#[test]
fn same_run_same_score_bit_for_bit() {
    let (score_a, hash_a) = score_run();
    let (score_b, hash_b) = score_run();
    assert_eq!(hash_a, hash_b, "the runs themselves must be identical");
    assert_eq!(score_a, score_b, "identical runs must score identically");
    // And the run actually produced something to score — an all-zero score
    // would make this test vacuous.
    assert!(score_a.samples == 20, "20 samples over 200 ticks");
    assert!(
        score_a.structures_peak > 0,
        "the bonding pack must form structures worth scoring"
    );
}
