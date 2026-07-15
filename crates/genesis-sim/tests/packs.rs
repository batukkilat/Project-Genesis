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
                dynamics: Default::default(),
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
fn gradient_sieve_field_value_sets_selection_strength() {
    // packs/gradient-sieve.ron adds exactly one env-gated absorb rule to the
    // sieve: where field 0 ≥ 0.6, the information floor rises from 0.2 to
    // 0.45. The executable causal claim: run the SAME pack on two worlds
    // differing only in the field's uniform value — gate closed (0.0) vs
    // gate open (1.0) — and the open-gate world must end with fewer
    // particles (extra absorptions) and fewer residents of the (0.2, 0.45)
    // information band the rule targets. Same rule list, same RNG stream
    // layout, same initial state: the field value is the entire difference
    // between the two universes.
    //
    // Deliberately NOT asserted: regional statistics inside one gradient
    // world. The standing regional distribution is an equilibrium the added
    // cull only partly controls — absorbed matter refuels info-rich
    // survivors whose SPLIT children (half of a ≥ 0.6 parent) land back
    // inside the band — so local selection strength does not map 1:1 onto
    // local standing statistics (observed while writing this test; recorded
    // in the 2026-07-15 findings doc). The uniform-field contrast pins the
    // mechanism without betting on that equilibrium.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packs");
    let pack = RulePack::load(&dir.join("gradient-sieve.ron")).unwrap();
    let run = |field_value: f32| {
        let mut config = small_config();
        config.particle_count = 3000;
        config.world_width = 548.0;
        config.world_height = 548.0;
        // The decay the shipped pairing (configs/gradient-sieve.ron) uses:
        // with no decay, IMPRINT lifts the whole world above the floor
        // within a few hundred ticks and no cull has anything to select on
        // (observed while writing this test).
        config.physics.information_decay = 0.02;
        config.initial.speed = Range::new(0.0, 0.5);
        config.env = EnvSpec {
            cols: 8,
            rows: 8,
            fields: vec![EnvFieldSpec {
                name: String::new(),
                init: FieldInit::Uniform(field_value),
                dynamics: Default::default(),
            }],
        };
        let mut sim = Simulation::with_rules(&config, RuleSet::compile(&pack));
        for _ in 0..600 {
            sim.tick();
        }
        let snap = sim.snapshot();
        let band = snap
            .particles
            .iter()
            .filter(|p| p.information >= 0.2 && p.information < 0.45)
            .count();
        (snap.particles.len(), band)
    };
    let (n_closed, band_closed) = run(0.0);
    let (n_open, band_open) = run(1.0);
    eprintln!(
        "gate closed: n={n_closed} band={band_closed} | \
         gate open: n={n_open} band={band_open}"
    );
    assert!(
        n_closed > 1000 && band_closed > 50,
        "closed-gate world unexpectedly empty (n={n_closed}, \
         band={band_closed}) — the contrast below would be vacuous"
    );
    assert!(
        n_open + 50 < n_closed,
        "the open gate must cost population through extra absorptions: \
         open {n_open} vs closed {n_closed}"
    );
    assert!(
        band_open + 20 < band_closed,
        "the open gate must thin the targeted information band: \
         open {band_open} vs closed {band_closed}"
    );
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
