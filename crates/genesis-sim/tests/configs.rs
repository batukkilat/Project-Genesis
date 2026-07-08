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
