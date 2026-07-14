//! Search step 2 (docs/research/search-design.md): the generation loop —
//! a basic evolutionary search over (config, pack) worlds, built entirely
//! out of shipped parts: `search::mutate` authors children, `score_run`
//! evaluates them, `search::fitness` ranks them. Nothing here executes a
//! world any other way than `genesis score` does.
//!
//! Determinism: the whole search is a pure function of (spec, build). Every
//! mutation stream is derived from (spec.seed, generation, individual);
//! selection orders by (fitness desc, id asc) with `total_cmp`, so ties
//! cannot depend on iteration order; the circuit breaker reads simulated
//! bond counts at sample boundaries, never wall time. Re-running the same
//! spec reproduces every file byte-for-byte.
//!
//! Reproducibility of any single step, without the search: children are
//! mutated from the parent files *as written to disk* (not the in-memory
//! copy), so `genesis mutate --config <parent.config> --rules <parent.pack>
//! --seed S --generation G --individual I` reproduces any child exactly,
//! and `genesis score` on a child's files reproduces its record.

use crate::search::{self, AncestryRecord, MutationOp};
use crate::sweep::score_run_capped;
use genesis_config::{ConfigError, RulePack, SimConfig};
use genesis_observer::{ObserverConfig, ScoreRecord};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Mean bond degree above which a run is flagged `condensed` on the
/// leaderboard: the world has welded into blob(s) (2026-07-13 baseline
/// sweep, finding 1). A mark for honest reading, never a penalty — fitness
/// already declines to reward condensation.
const CONDENSED_MEAN_DEGREE: f64 = 50.0;

fn default_sigma() -> f32 {
    0.3
}

fn default_confirm_top() -> u32 {
    3
}

fn default_mutations_per_child() -> u32 {
    1
}

/// One corpus seed: a named (config, pack) pair the search starts from.
/// Generation 0 is exactly this list, copied into the search directory and
/// screened like any child.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedSpec {
    /// Label for the leaderboard; must be filename-safe.
    pub name: String,
    /// RON config; the default config when omitted.
    #[serde(default)]
    pub config: Option<PathBuf>,
    /// RON rule pack. Mutation needs content to mutate.
    pub rules: PathBuf,
}

/// A search: an evolutionary loop over worlds. RON on disk.
///
/// Budget shape (design doc + baseline finding 5): every individual is
/// *screened* at `screen_ticks`; selection runs on screen fitness; after the
/// last generation the all-time top `confirm_top` re-run once at
/// `confirm_ticks` — the long-horizon records the exit criterion compares
/// against the shipped corpus. Confirmation at the end instead of per
/// generation is a deliberate budget decision (a single bonded 20k-tick run
/// costs minutes to hours; per-generation confirmation would dwarf the
/// search itself).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchSpec {
    /// Search master seed: drives every mutation stream. Distinct from the
    /// simulation seeds — worlds keep whatever seed their config declares.
    pub seed: u64,
    /// Child generations to run (generation 0 is the seed corpus).
    pub generations: u32,
    /// Children per generation.
    pub population: u32,
    /// Truncation selection: parents are the top-k of *everything evaluated
    /// so far* — implicit elitism, a parent is only displaced by a child
    /// that actually beats it.
    pub survivors: u32,
    /// Mutation magnitude (values scale by exp(±sigma)).
    #[serde(default = "default_sigma")]
    pub sigma: f32,
    /// Mutations applied per child, drawn sequentially from the child's one
    /// derivation stream — exactly `genesis mutate --steps`, so any child
    /// stays hand-reproducible in one command. Default 1 (search-01
    /// behavior); raise it to take bolder steps when single mutations
    /// plateau (search-01 finding 1: a homeostatic regime barely moves
    /// under one σ=0.3 jitter).
    #[serde(default = "default_mutations_per_child")]
    pub mutations_per_child: u32,
    /// Screening horizon: every individual simulates this many ticks.
    pub screen_ticks: u64,
    /// Observer sample cadence for every evaluation, in ticks.
    pub every: u64,
    /// Confirmation horizon for the all-time top `confirm_top`; no
    /// confirmation stage when omitted.
    #[serde(default)]
    pub confirm_ticks: Option<u64>,
    #[serde(default = "default_confirm_top")]
    pub confirm_top: u32,
    /// RON observer config applied to every evaluation (defaults when
    /// omitted) — one observer per search, so fitness is comparable.
    #[serde(default)]
    pub observer: Option<PathBuf>,
    /// Deterministic circuit breaker: an evaluation stops at the first
    /// sample counting more bonds than this, scoring what happened up to
    /// there with fitness 0. Bond count is the wall-time driver (baseline
    /// finding 5), and unlike a wall-clock cap it is simulated state — the
    /// breaker fires identically on every machine, keeping the search
    /// trajectory replayable.
    #[serde(default)]
    pub bond_cap: Option<usize>,
    /// Circuit breaker for the confirmation stage; falls back to `bond_cap`
    /// when omitted. The cap is a per-evaluation cost bound, and bonds grow
    /// with the horizon — a cap sized for the screen spuriously kills every
    /// longer confirmation run (search-01 finding 3: both 6k-tick confirms
    /// tripped the 3k-sized cap, by arithmetic rather than bad luck).
    #[serde(default)]
    pub confirm_bond_cap: Option<usize>,
    /// The seed corpus. At least one entry.
    pub seeds: Vec<SeedSpec>,
}

impl SearchSpec {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let spec: SearchSpec =
            ron::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.seeds.is_empty() {
            return Err(ConfigError::Invalid("search has no seeds".into()));
        }
        if self.generations == 0 || self.population == 0 || self.survivors == 0 {
            return Err(ConfigError::Invalid(
                "generations, population, and survivors must be at least 1".into(),
            ));
        }
        if self.screen_ticks == 0 || self.every == 0 {
            return Err(ConfigError::Invalid(
                "screen_ticks and every must be at least 1".into(),
            ));
        }
        if self.confirm_ticks == Some(0) || (self.confirm_ticks.is_some() && self.confirm_top == 0)
        {
            return Err(ConfigError::Invalid(
                "confirm_ticks and confirm_top must be at least 1 when confirming".into(),
            ));
        }
        if !(self.sigma > 0.0 && self.sigma.is_finite()) {
            return Err(ConfigError::Invalid(
                "sigma must be positive and finite".into(),
            ));
        }
        if self.mutations_per_child == 0 {
            return Err(ConfigError::Invalid(
                "mutations_per_child must be at least 1".into(),
            ));
        }
        let mut names: Vec<&str> = self.seeds.iter().map(|s| s.name.as_str()).collect();
        names.sort_unstable();
        if let Some(w) = names.windows(2).find(|w| w[0] == w[1]) {
            return Err(ConfigError::Invalid(format!(
                "duplicate seed name {:?}",
                w[0]
            )));
        }
        for name in names {
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            {
                return Err(ConfigError::Invalid(format!(
                    "seed name {name:?} must be non-empty and use only [A-Za-z0-9._-]"
                )));
            }
        }
        Ok(())
    }
}

/// One evaluated individual, alive in the selection pool.
struct Individual {
    id: String,
    /// Generation this individual's files live under.
    generation: u64,
    /// Seed name for generation 0, parent id otherwise (leaderboard lineage
    /// column; the ancestry sidecar holds the full story).
    origin: String,
    ops: Vec<MutationOp>,
    /// Authoring content as loaded back from this individual's own files —
    /// the on-disk artifact is the authority children mutate from.
    config: SimConfig,
    pack: RulePack,
    fitness: f64,
    capped: bool,
    record: ScoreRecord,
}

/// Machine-readable search outcome, written as `summary.ron` next to the
/// leaderboard. One row per individual, in id order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchSummary {
    pub search_seed: u64,
    pub individuals: Vec<IndividualSummary>,
    /// Ids of the confirmed individuals, leaderboard order.
    pub confirmed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndividualSummary {
    pub id: String,
    /// Seed name (generation 0) or parent id.
    pub origin: String,
    pub fitness: f64,
    /// Headline exit-criterion scalar (max persistence × complexity).
    pub persistence_complexity: f64,
    /// The bond-cap breaker fired; fitness is 0 by fiat.
    pub capped: bool,
    /// Mean bond degree exceeded the condensation threshold.
    pub condensed: bool,
    pub state_hash: u64,
}

impl SearchSummary {
    pub fn to_ron(&self) -> Result<String, ConfigError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| ConfigError::Parse(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        ron::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))
    }
}

fn condensed(record: &ScoreRecord) -> bool {
    let s = &record.score;
    s.particles_final > 0
        && 2.0 * s.bonds_final as f64 / s.particles_final as f64 > CONDENSED_MEAN_DEGREE
}

/// Pool indices ranked best-first: fitness descending, id ascending as the
/// total-order tiebreak (f64 via total_cmp — no ordering left to chance).
fn ranked_indices(pool: &[(f64, String)]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..pool.len()).collect();
    idx.sort_by(|&a, &b| {
        pool[b]
            .0
            .total_cmp(&pool[a].0)
            .then_with(|| pool[a].1.cmp(&pool[b].1))
    });
    idx
}

fn rank_pool(pool: &[Individual]) -> Vec<usize> {
    let keyed: Vec<(f64, String)> = pool.iter().map(|i| (i.fitness, i.id.clone())).collect();
    ranked_indices(&keyed)
}

/// Short human form of one operator for the leaderboard.
fn op_brief(op: &MutationOp) -> String {
    match op {
        MutationOp::JitterConfig { field, old, new } => {
            format!("jitter {field} {old:.4}→{new:.4}")
        }
        MutationOp::JitterRule {
            rule,
            field,
            old,
            new,
        } => format!("jitter r{rule}.{field} {old:.4}→{new:.4}"),
        MutationOp::DropRule { rule } => format!("drop r{rule}"),
        MutationOp::DuplicateRule {
            source, new_index, ..
        } => format!("dup r{source}→r{new_index}"),
        MutationOp::RewireCondition {
            rule,
            side,
            from,
            to,
        } => format!("rewire r{rule}.{side} {from}→{to}"),
    }
}

/// All of an individual's operators, in application order; "—" for seeds.
fn ops_brief(ops: &[MutationOp]) -> String {
    if ops.is_empty() {
        "—".into()
    } else {
        ops.iter().map(op_brief).collect::<Vec<_>>().join("; ")
    }
}

/// Write one individual's authoring files + ancestry sidecar into the
/// generation directory, then evaluate it through the one scoring path.
/// The stored (config, pack) are loaded back from the written files, so
/// the on-disk artifact — not the in-memory intermediate — is what both
/// the evaluation and any future child see.
#[allow(clippy::too_many_arguments)]
fn write_and_eval(
    spec: &SearchSpec,
    out: &Path,
    generation: u64,
    individual: u64,
    origin: String,
    parent: Option<String>,
    ops: Vec<MutationOp>,
    config: &SimConfig,
    pack: &RulePack,
    observer: ObserverConfig,
    verbose: bool,
) -> Result<Individual, Box<dyn std::error::Error>> {
    let id = format!("g{generation:03}-i{individual:03}");
    let rel_dir = format!("g{generation:03}");
    let gen_dir = out.join(&rel_dir);
    std::fs::create_dir_all(&gen_dir)?;

    // Belt and braces, same as `genesis mutate`: operators repair-clamp,
    // but the artifact must pass the exact loaders the scorer uses.
    config.validate()?;
    pack.validate()?;

    let config_path = gen_dir.join(format!("{id}.config.ron"));
    let pack_path = gen_dir.join(format!("{id}.pack.ron"));
    config.save(&config_path)?;
    pack.save(&pack_path)?;

    let ancestry = AncestryRecord {
        id: id.clone(),
        parent,
        ops: ops.clone(),
        search_seed: spec.seed,
        generation,
        individual,
        config: format!("{rel_dir}/{id}.config.ron"),
        rules: format!("{rel_dir}/{id}.pack.ron"),
    };
    std::fs::write(
        gen_dir.join(format!("{id}.ancestry.ron")),
        ancestry.to_ron()?,
    )?;

    let (mut record, capped) = score_run_capped(
        &Some(config_path.clone()),
        &Some(pack_path.clone()),
        &None,
        spec.screen_ticks,
        spec.every,
        observer,
        spec.bond_cap,
    )?;
    // Stamp paths relative to the search directory: the paths are
    // documentation (labels above the engine), and a search directory is a
    // committable artifact — it must not bake in where it was produced.
    record.config = Some(ancestry.config.clone());
    record.rules = Some(ancestry.rules.clone());
    std::fs::write(gen_dir.join(format!("{id}.score.ron")), record.to_ron()?)?;

    // A capped run is dead to selection; its ancestry and record remain.
    let fitness = if capped {
        0.0
    } else {
        search::fitness(&record.score)
    };
    if verbose {
        println!(
            "  {id}  fitness {fitness:>8.3}  score {:>10.2}  structures {:>5}  \
             bonds {:>7}{}{}  [{}]",
            record.score.persistence_complexity,
            record.score.structures_final,
            record.score.bonds_final,
            if capped { "  CAPPED" } else { "" },
            if condensed(&record) {
                "  CONDENSED"
            } else {
                ""
            },
            ops_brief(&ops),
        );
    }

    Ok(Individual {
        id,
        generation,
        origin,
        ops,
        config: SimConfig::load(&config_path)?,
        pack: RulePack::load(&pack_path)?,
        fitness,
        capped,
        record,
    })
}

/// Run a whole search. Everything lands under `out`:
/// `g###/i###.{config,pack,ancestry,score}.ron` per individual,
/// `<id>.confirm.score.ron` for confirmed finalists, `leaderboard.md`,
/// and `summary.ron`. Returns the summary.
pub fn run_search(
    spec: &SearchSpec,
    out: &Path,
    observer: ObserverConfig,
    verbose: bool,
) -> Result<SearchSummary, Box<dyn std::error::Error>> {
    spec.validate()?;
    std::fs::create_dir_all(out)?;
    let mut pool: Vec<Individual> = Vec::new();

    // Generation 0: the seed corpus, copied into the search directory so a
    // finished search is a self-contained artifact.
    if verbose {
        println!("generation 0 (seed corpus, {} seeds)", spec.seeds.len());
    }
    for (i, seed) in spec.seeds.iter().enumerate() {
        let config = match &seed.config {
            Some(p) => SimConfig::load(p)?,
            None => SimConfig::default(),
        };
        let pack = RulePack::load(&seed.rules)?;
        pool.push(write_and_eval(
            spec,
            out,
            0,
            i as u64,
            seed.name.clone(),
            None,
            Vec::new(),
            &config,
            &pack,
            observer,
            verbose,
        )?);
    }

    for g in 1..=spec.generations as u64 {
        let ranking = rank_pool(&pool);
        let parents: Vec<usize> = ranking.into_iter().take(spec.survivors as usize).collect();
        if verbose {
            let names: Vec<&str> = parents.iter().map(|&p| pool[p].id.as_str()).collect();
            println!("generation {g} (parents: {})", names.join(", "));
        }
        let mut children = Vec::new();
        for i in 0..spec.population as u64 {
            let parent = &pool[parents[i as usize % parents.len()]];
            let mut config = parent.config.clone();
            let mut pack = parent.pack.clone();
            let mut rng = search::mutation_rng(spec.seed, g, i);
            let ops: Vec<MutationOp> = (0..spec.mutations_per_child)
                .map(|_| search::mutate(&mut config, &mut pack, &mut rng, spec.sigma))
                .collect();
            children.push(write_and_eval(
                spec,
                out,
                g,
                i,
                parent.id.clone(),
                Some(parent.id.clone()),
                ops,
                &config,
                &pack,
                observer,
                verbose,
            )?);
        }
        pool.extend(children);
    }

    // Confirmation: the all-time top-k re-run at the long horizon — the
    // records the exit criterion compares against the shipped corpus.
    let ranking = rank_pool(&pool);
    let mut confirmed_ids = Vec::new();
    if let Some(confirm_ticks) = spec.confirm_ticks {
        for &p in ranking.iter().take(spec.confirm_top as usize) {
            let ind = &pool[p];
            if verbose {
                println!("confirming {} at {confirm_ticks} ticks", ind.id);
            }
            let gen_dir = out.join(format!("g{:03}", ind.generation));
            let (mut record, capped) = score_run_capped(
                &Some(gen_dir.join(format!("{}.config.ron", ind.id))),
                &Some(gen_dir.join(format!("{}.pack.ron", ind.id))),
                &None,
                confirm_ticks,
                spec.every,
                observer,
                spec.confirm_bond_cap.or(spec.bond_cap),
            )?;
            record.config = Some(format!("g{:03}/{}.config.ron", ind.generation, ind.id));
            record.rules = Some(format!("g{:03}/{}.pack.ron", ind.generation, ind.id));
            std::fs::write(
                out.join(format!("{}.confirm.score.ron", ind.id)),
                record.to_ron()?,
            )?;
            if verbose {
                println!(
                    "  confirmed {}  score {:>10.2}  fitness {:>8.3}{}",
                    ind.id,
                    record.score.persistence_complexity,
                    if capped {
                        0.0
                    } else {
                        search::fitness(&record.score)
                    },
                    if capped { "  CAPPED" } else { "" },
                );
            }
            confirmed_ids.push(ind.id.clone());
        }
    }

    // Leaderboard (best-first) + machine summary (id order).
    let mut leaderboard = String::from(
        "| rank | id | origin | op | fitness | score (pers x cplx) | structures fin/peak \
         | lifetime peak | info final | bonds | flags | state hash |\n\
         |---:|---|---|---|---:|---:|---:|---:|---:|---:|---|---|\n",
    );
    for (rank, &p) in ranking.iter().enumerate() {
        let ind = &pool[p];
        let s = &ind.record.score;
        let mut flags = Vec::new();
        if ind.capped {
            flags.push("capped");
        }
        if condensed(&ind.record) {
            flags.push("condensed");
        }
        leaderboard.push_str(&format!(
            "| {} | {} | {} | {} | {:.3} | {:.2} | {}/{} | {} | {:.2} | {} | {} | {:#018x} |\n",
            rank + 1,
            ind.id,
            ind.origin,
            ops_brief(&ind.ops),
            ind.fitness,
            s.persistence_complexity,
            s.structures_final,
            s.structures_peak,
            s.lifetime_peak,
            s.information_final,
            s.bonds_final,
            if flags.is_empty() {
                "—".to_string()
            } else {
                flags.join(" ")
            },
            ind.record.state_hash,
        ));
    }
    std::fs::write(out.join("leaderboard.md"), &leaderboard)?;

    let mut individuals: Vec<IndividualSummary> = pool
        .iter()
        .map(|ind| IndividualSummary {
            id: ind.id.clone(),
            origin: ind.origin.clone(),
            fitness: ind.fitness,
            persistence_complexity: ind.record.score.persistence_complexity,
            capped: ind.capped,
            condensed: condensed(&ind.record),
            state_hash: ind.record.state_hash,
        })
        .collect();
    individuals.sort_by(|a, b| a.id.cmp(&b.id));
    let summary = SearchSummary {
        search_seed: spec.seed,
        individuals,
        confirmed: confirmed_ids,
    };
    let summary_path = out.join("summary.ron");
    std::fs::write(&summary_path, summary.to_ron()?)?;
    // Read-back check, the mutate-CLI precedent: the sidecar a later audit
    // will trust must load as written.
    if SearchSummary::load(&summary_path)? != summary {
        return Err("search summary did not round-trip through disk".into());
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_config::Range;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("genesis-evolve-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A cheap world: few particles, small torus, information present so
    /// info-conditioned rules can fire.
    fn tiny_config() -> SimConfig {
        SimConfig {
            particle_count: 300,
            world_width: 256.0,
            world_height: 256.0,
            initial: genesis_config::InitialRanges {
                matter: Range::new(0.1, 1.0),
                energy: Range::new(0.0, 1.0),
                information: Range::new(0.0, 1.0),
                speed: Range::new(0.0, 2.0),
            },
            ..SimConfig::default()
        }
    }

    fn tiny_spec(dir: &Path) -> SearchSpec {
        let config_path = dir.join("tiny.config.ron");
        tiny_config().save(&config_path).unwrap();
        SearchSpec {
            seed: 11,
            generations: 2,
            population: 3,
            survivors: 2,
            sigma: 0.3,
            mutations_per_child: 1,
            screen_ticks: 60,
            every: 20,
            confirm_ticks: Some(90),
            confirm_top: 2,
            observer: None,
            bond_cap: None,
            confirm_bond_cap: None,
            seeds: vec![SeedSpec {
                name: "tiny-chains".into(),
                config: Some(config_path),
                rules: repo_root().join("packs/chains.ron"),
            }],
        }
    }

    #[test]
    fn spec_validation_rejects_degenerate_values() {
        let dir = temp_dir("validate");
        let good = tiny_spec(&dir);
        assert!(good.validate().is_ok());
        let mut s = good.clone();
        s.seeds.clear();
        assert!(s.validate().is_err(), "no seeds");
        let mut s = good.clone();
        s.population = 0;
        assert!(s.validate().is_err(), "zero population");
        let mut s = good.clone();
        s.survivors = 0;
        assert!(s.validate().is_err(), "zero survivors");
        let mut s = good.clone();
        s.every = 0;
        assert!(s.validate().is_err(), "zero cadence");
        let mut s = good.clone();
        s.confirm_ticks = Some(0);
        assert!(s.validate().is_err(), "zero confirm horizon");
        let mut s = good.clone();
        s.sigma = 0.0;
        assert!(s.validate().is_err(), "zero sigma");
        let mut s = good.clone();
        s.mutations_per_child = 0;
        assert!(s.validate().is_err(), "zero mutations per child");
        let mut s = good.clone();
        s.seeds[0].name = "has space".into();
        assert!(s.validate().is_err(), "names become filenames");
        let mut s = good.clone();
        s.seeds.push(s.seeds[0].clone());
        assert!(s.validate().is_err(), "duplicate names");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ranking_is_total_ordered_fitness_then_id() {
        let pool = vec![
            (1.0, "b".to_string()),
            (2.0, "c".to_string()),
            (2.0, "a".to_string()),
            (0.0, "d".to_string()),
        ];
        assert_eq!(ranked_indices(&pool), vec![2, 1, 0, 3]);
    }

    /// The whole loop, twice, must be byte-identical: summary, leaderboard,
    /// every per-individual artifact. This is the "replaying a whole search
    /// = same seed, same spec" property the ancestry design promises.
    #[test]
    fn tiny_search_is_deterministic_end_to_end() {
        let dir = temp_dir("determinism");
        let spec = tiny_spec(&dir);
        let run = |sub: &str| {
            let out = dir.join(sub);
            let summary = run_search(&spec, &out, ObserverConfig::default(), false).unwrap();
            (summary, out)
        };
        let (summary_a, out_a) = run("a");
        let (summary_b, out_b) = run("b");
        assert_eq!(summary_a, summary_b, "summaries must match");
        // 1 seed + 2 generations x 3 children.
        assert_eq!(summary_a.individuals.len(), 7);
        assert_eq!(summary_a.confirmed.len(), 2);
        assert_eq!(
            SearchSummary::load(&out_a.join("summary.ron")).unwrap(),
            summary_a,
            "the written summary must round-trip"
        );

        for rel in [
            "leaderboard.md",
            "summary.ron",
            "g001/g001-i000.ancestry.ron",
            "g002/g002-i002.score.ron",
            "g001/g001-i001.pack.ron",
        ] {
            let a = std::fs::read(out_a.join(rel)).unwrap();
            let b = std::fs::read(out_b.join(rel)).unwrap();
            assert_eq!(a, b, "{rel} must be byte-identical across reruns");
        }

        // Every child's ancestry names a parent that exists in the pool and
        // points at files that load.
        let ids: std::collections::BTreeSet<String> =
            summary_a.individuals.iter().map(|i| i.id.clone()).collect();
        for g in 1..=2 {
            for i in 0..3 {
                let rec = AncestryRecord::load(
                    &out_a.join(format!("g{g:03}/g{g:03}-i{i:03}.ancestry.ron")),
                )
                .unwrap();
                let parent = rec.parent.expect("children have parents");
                assert!(ids.contains(&parent), "parent {parent} must exist");
                assert_eq!(rec.ops.len(), 1, "children carry their operator");
                SimConfig::load(&out_a.join(&rec.config)).unwrap();
                RulePack::load(&out_a.join(&rec.rules)).unwrap();
            }
        }
        // Confirmed records exist for the confirmed ids.
        for id in &summary_a.confirmed {
            let record = ScoreRecord::load(&out_a.join(format!("{id}.confirm.score.ron"))).unwrap();
            assert_eq!(record.ticks, 90, "confirmation ran the long horizon");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Multi-mutation children: every child's sidecar carries exactly
    /// `mutations_per_child` operators, and re-applying them via the same
    /// stream on the parent's on-disk files reproduces the child's files —
    /// the `genesis mutate --steps` equivalence the spec field promises.
    #[test]
    fn multi_mutation_children_reproduce_from_parent_artifacts() {
        let dir = temp_dir("multimut");
        let mut spec = tiny_spec(&dir);
        spec.mutations_per_child = 3;
        spec.generations = 1;
        spec.confirm_ticks = None;
        let out = dir.join("out");
        let summary = run_search(&spec, &out, ObserverConfig::default(), false).unwrap();
        assert_eq!(summary.individuals.len(), 4); // 1 seed + 3 children

        for i in 0..3u64 {
            let rec =
                AncestryRecord::load(&out.join(format!("g001/g001-i{i:03}.ancestry.ron"))).unwrap();
            assert_eq!(rec.ops.len(), 3, "children carry all three operators");
            let parent_rec =
                AncestryRecord::load(&out.join("g000/g000-i000.ancestry.ron")).unwrap();
            let mut config = SimConfig::load(&out.join(&parent_rec.config)).unwrap();
            let mut pack = RulePack::load(&out.join(&parent_rec.rules)).unwrap();
            let mut rng = search::mutation_rng(spec.seed, 1, i);
            let replayed: Vec<MutationOp> = (0..3)
                .map(|_| search::mutate(&mut config, &mut pack, &mut rng, spec.sigma))
                .collect();
            assert_eq!(replayed, rec.ops, "the op chain replays exactly");
            let child_pack = RulePack::load(&out.join(&rec.rules)).unwrap();
            let child_config = SimConfig::load(&out.join(&rec.config)).unwrap();
            assert_eq!(
                ron::ser::to_string(&pack).unwrap(),
                ron::ser::to_string(&child_pack).unwrap(),
                "replayed pack matches the committed child"
            );
            assert_eq!(
                ron::ser::to_string(&config).unwrap(),
                ron::ser::to_string(&child_config).unwrap(),
                "replayed config matches the committed child"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `confirm_bond_cap` overrides `bond_cap` for the confirmation stage
    /// only: a screen-sized cap kills every screen, while the confirmation
    /// of the same world runs its full horizon under the larger cap
    /// (search-01 finding 3 — the cap must scale with the horizon).
    #[test]
    fn confirm_bond_cap_overrides_screen_cap() {
        let dir = temp_dir("confirmcap");
        // The eager-bonding world from the bond-cap test: bonding is certain
        // by the first sample, so a cap of 0 trips every screen.
        let mut pack = RulePack::load(&repo_root().join("packs/chains.ron")).unwrap();
        let max_radius = SimConfig::default().physics.interaction_radius;
        pack.rules.retain(|r| r.bond_create.is_some());
        assert!(!pack.rules.is_empty());
        for r in &mut pack.rules {
            r.probability = 1.0;
            r.radius = max_radius;
            r.self_cond = Default::default();
            r.other_cond = Default::default();
        }
        let pack_path = dir.join("eager.pack.ron");
        pack.save(&pack_path).unwrap();
        let mut config = tiny_config();
        config.particle_count = 600;
        let config_path = dir.join("dense.config.ron");
        config.save(&config_path).unwrap();

        let spec = SearchSpec {
            seed: 5,
            generations: 1,
            population: 1,
            survivors: 1,
            sigma: 0.3,
            mutations_per_child: 1,
            screen_ticks: 60,
            every: 10,
            confirm_ticks: Some(90),
            confirm_top: 1,
            observer: None,
            bond_cap: Some(0),
            confirm_bond_cap: Some(usize::MAX),
            seeds: vec![SeedSpec {
                name: "eager".into(),
                config: Some(config_path),
                rules: pack_path,
            }],
        };
        let out = dir.join("out");
        let summary = run_search(&spec, &out, ObserverConfig::default(), false).unwrap();

        let seed_row = &summary.individuals[0];
        assert!(seed_row.capped, "the screen must trip the screen cap");
        assert_eq!(summary.confirmed.len(), 1);
        let record =
            ScoreRecord::load(&out.join(format!("{}.confirm.score.ron", summary.confirmed[0])))
                .unwrap();
        assert_eq!(
            record.ticks, 90,
            "confirmation must run its full horizon under its own cap"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The bond-cap breaker: with a cap of 0 every run that forms any bond
    /// stops at its first sample, scores fitness 0, and the whole search
    /// still completes and reproduces.
    #[test]
    fn bond_cap_kills_bonding_runs_deterministically() {
        let dir = temp_dir("bondcap");
        // Make bonding certain and immediate: strip every gate from the
        // chains pack and fire its rules at probability 1 in a dense world.
        let mut pack = RulePack::load(&repo_root().join("packs/chains.ron")).unwrap();
        let max_radius = SimConfig::default().physics.interaction_radius;
        // Keep only the bond-creating rule — an eager break rule would undo
        // every bond in the same breath — and strip its gates.
        pack.rules.retain(|r| r.bond_create.is_some());
        assert!(!pack.rules.is_empty());
        for r in &mut pack.rules {
            r.probability = 1.0;
            r.radius = max_radius;
            r.self_cond = Default::default();
            r.other_cond = Default::default();
        }
        let pack_path = dir.join("eager.pack.ron");
        pack.save(&pack_path).unwrap();
        let mut config = tiny_config();
        config.particle_count = 600;
        let config_path = dir.join("dense.config.ron");
        config.save(&config_path).unwrap();

        let spec = SearchSpec {
            seed: 5,
            generations: 1,
            population: 2,
            survivors: 1,
            sigma: 0.3,
            mutations_per_child: 1,
            screen_ticks: 100,
            every: 10,
            confirm_ticks: None,
            confirm_top: 1,
            observer: None,
            bond_cap: Some(0),
            confirm_bond_cap: None,
            seeds: vec![SeedSpec {
                name: "eager".into(),
                config: Some(config_path),
                rules: pack_path,
            }],
        };
        let summary_a =
            run_search(&spec, &dir.join("a"), ObserverConfig::default(), false).unwrap();
        let summary_b =
            run_search(&spec, &dir.join("b"), ObserverConfig::default(), false).unwrap();
        assert_eq!(summary_a, summary_b, "capped searches must reproduce");

        let seed_row = &summary_a.individuals[0];
        assert!(seed_row.capped, "the eager pack must trip a cap of 0");
        assert_eq!(seed_row.fitness, 0.0, "capped runs are dead to selection");
        let record = ScoreRecord::load(&dir.join("a/g000/g000-i000.score.ron")).unwrap();
        assert!(
            record.ticks < 100,
            "the record stamps the ticks actually simulated ({})",
            record.ticks
        );
        assert_eq!(record.ticks % 10, 0, "the breaker fires at a sample tick");
        std::fs::remove_dir_all(&dir).ok();
    }
}
