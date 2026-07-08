//! Every committed rule pack is a shipped artifact: it must load, validate,
//! compile, pass simulation-assembly checks against the default physics, and
//! run deterministically. Without this test a pack typo only surfaces when
//! someone runs that pack.

use genesis_config::{EnvFieldSpec, EnvSpec, FieldInit, Range, RulePack, SimConfig};
use genesis_sim::Simulation;
use genesis_sim::interact::RuleSet;
use std::path::PathBuf;

fn committed_packs() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packs");
    let mut packs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "ron"))
        .collect();
    packs.sort();
    assert!(
        packs.len() >= 7,
        "expected at least the 7 committed packs, found {}",
        packs.len()
    );
    packs
}

/// Small world with every quantity live, so info- and matter-conditioned
/// rules can actually fire.
fn small_config() -> SimConfig {
    let mut config = SimConfig {
        seed: 7,
        particle_count: 200,
        world_width: 64.0,
        world_height: 64.0,
        ..Default::default()
    };
    config.initial.information = Range::new(0.0, 1.0);
    config
}

/// The environment a pack declares it needs: one gradient field per index it
/// references. Env-free packs get an env-free config, exactly as before.
fn env_for(pack: &RulePack) -> EnvSpec {
    let needed = pack
        .rules
        .iter()
        .flat_map(|r| r.env_cond.iter())
        .map(|e| e.field + 1)
        .max()
        .unwrap_or(0);
    EnvSpec {
        cols: 8,
        rows: 8,
        fields: (0..needed)
            .map(|_| EnvFieldSpec {
                name: String::new(),
                init: FieldInit::GradientX { lo: 0.0, hi: 1.0 },
            })
            .collect(),
    }
}

#[test]
fn every_committed_pack_loads_compiles_and_assembles() {
    for path in committed_packs() {
        let pack = RulePack::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            !pack.rules.is_empty(),
            "{}: pack has no rules",
            path.display()
        );
        let mut config = small_config();
        config.env = env_for(&pack);
        let rules = RuleSet::compile(&pack);
        // Assembly runs assert_valid (NaN, radius-vs-grid, and env-field
        // checks) — a panic here means the pack cannot run on default physics
        // plus the environment it declares it needs.
        let _ = Simulation::with_rules(&config, rules);
    }
}

#[test]
fn every_committed_pack_is_deterministic() {
    for path in committed_packs() {
        let pack = RulePack::load(&path).unwrap();
        let mut config = small_config();
        config.env = env_for(&pack);
        let run = || {
            let rules = RuleSet::compile(&pack);
            let mut sim = Simulation::with_rules(&config, rules);
            for _ in 0..30 {
                sim.tick();
            }
            sim.state_hash()
        };
        assert_eq!(run(), run(), "{}: replay diverged", path.display());
    }
}
