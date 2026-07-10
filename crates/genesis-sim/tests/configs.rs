//! Every committed config in `configs/` is a shipped artifact: it must load,
//! validate, and run deterministically (including thread-count invariance when
//! it enables LOD). Without this a config typo — a malformed LOD ladder, an
//! out-of-range physics param — only surfaces when someone runs that file.

use genesis_config::SimConfig;
use genesis_sim::Simulation;
use std::path::PathBuf;

fn committed_configs() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../configs");
    let mut configs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "ron"))
        .collect();
    configs.sort();
    assert!(
        !configs.is_empty(),
        "no committed configs found in {}",
        dir.display()
    );
    configs
}

/// Load a shipped config but shrink it to a fast, live test size. The world
/// and physics (including any LOD policy) are preserved; only the particle
/// count is reduced so the test stays quick.
fn load_small(path: &std::path::Path) -> SimConfig {
    let mut config = SimConfig::load(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    config.particle_count = 400;
    config
}

#[test]
fn every_committed_action_script_loads_and_validates() {
    // scripts/ is shipped content like configs/ and packs/: every script must
    // load and pass structural validation. (Field indices are checked against
    // a config at assembly; the canonical pairing is exercised in the sim
    // test suite.)
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read scripts/: {e}")) {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "ron") {
            let script = genesis_config::ActionScript::load(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(
                !script.actions.is_empty(),
                "{}: script has no actions",
                path.display()
            );
            checked += 1;
        }
    }
    assert!(checked >= 1, "expected at least one shipped script");
}

#[test]
fn every_committed_config_loads_and_validates() {
    for path in committed_configs() {
        // SimConfig::load runs validate(), so a bad LOD ladder or physics
        // param fails right here with the file named.
        let _ = load_small(&path);
    }
}

#[test]
fn every_committed_config_is_deterministic() {
    for path in committed_configs() {
        let config = load_small(&path);
        let run = || {
            let mut sim = Simulation::new(&config);
            for _ in 0..40 {
                sim.tick();
            }
            sim.state_hash()
        };
        assert_eq!(run(), run(), "{}: replay diverged", path.display());
    }
}

#[test]
fn full_stack_pairing_assembles_and_replays() {
    // configs/full-stack.ron + scripts/full-stack.ron are shipped as a
    // *pair* (the kitchen-sink determinism scenario): the script's field
    // indices only validate against the config at assembly, so the sweep
    // tests above can't catch a mismatch between the two files. Run the
    // pair past its last stamped action and check replay + the injected
    // payloads arriving.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = load_small(&root.join("configs/full-stack.ron"));
    let script = genesis_config::ActionScript::load(&root.join("scripts/full-stack.ron")).unwrap();
    let last_tick = script.actions.iter().map(|a| a.tick).max().unwrap();
    let run = || {
        let mut sim = Simulation::with_rules_and_actions(
            &config,
            genesis_sim::interact::RuleSet::default(),
            script.clone(),
        );
        for _ in 0..=last_tick {
            sim.tick();
        }
        (sim.state_hash(), sim.particle_count())
    };
    let (hash_a, count_a) = run();
    let (hash_b, _) = run();
    assert_eq!(hash_a, hash_b, "full-stack pairing replay diverged");
    let payload_total: usize = script
        .actions
        .iter()
        .map(|a| match a.action {
            genesis_config::ActionKind::Impact { payload, .. }
            | genesis_config::ActionKind::Rift { payload, .. } => payload.count as usize,
            _ => 0,
        })
        .sum();
    assert_eq!(
        count_a,
        400 + payload_total,
        "impact + rift payloads must have arrived exactly"
    );
}

#[test]
fn full_stack_pairing_survives_threads_and_mid_script_resume() {
    // The scenario's whole point is cross-feature interaction — rifts and
    // impacts landing mid-LOD-stride on a dynamic env — so determinism must
    // hold where those features can actually disagree: across thread counts
    // and across a save taken while actions are still pending (night review
    // 2026-07-10; the replay test above covers neither).
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = load_small(&root.join("configs/full-stack.ron"));
    let script = genesis_config::ActionScript::load(&root.join("scripts/full-stack.ron")).unwrap();
    let last_tick = script.actions.iter().map(|a| a.tick).max().unwrap();
    let split = script.actions.iter().map(|a| a.tick).min().unwrap() + 1;
    assert!(split < last_tick, "script needs actions on distinct ticks");

    let fresh = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            let mut sim = Simulation::with_rules_and_actions(
                &config,
                genesis_sim::interact::RuleSet::default(),
                script.clone(),
            );
            for _ in 0..=last_tick {
                sim.tick();
            }
            sim.state_hash()
        })
    };
    let reference = fresh(1);
    assert_eq!(
        reference,
        fresh(4),
        "full-stack pairing broke thread invariance"
    );

    // Save after the first action applied but with later ones pending; the
    // resumed run must land on the identical final state.
    let mut sim = Simulation::with_rules_and_actions(
        &config,
        genesis_sim::interact::RuleSet::default(),
        script.clone(),
    );
    for _ in 0..split {
        sim.tick();
    }
    let snap = sim.snapshot();
    assert!(
        !snap.pending_actions.is_empty(),
        "the save must capture a non-empty pending queue"
    );
    let mut resumed = Simulation::from_snapshot(&snap);
    for _ in split..=last_tick {
        resumed.tick();
    }
    assert_eq!(
        reference,
        resumed.state_hash(),
        "full-stack pairing broke mid-script save/resume"
    );
}

#[test]
fn lod_enabled_configs_are_thread_count_invariant() {
    for path in committed_configs() {
        let config = load_small(&path);
        if !config.lod.enabled {
            continue;
        }
        let run = |threads: usize| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                let mut sim = Simulation::new(&config);
                for _ in 0..40 {
                    sim.tick();
                }
                sim.state_hash()
            })
        };
        assert_eq!(
            run(1),
            run(4),
            "{}: LOD-enabled config broke thread invariance",
            path.display()
        );
    }
}
