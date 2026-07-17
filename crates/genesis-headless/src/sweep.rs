//! Sweep driver — Phase 6.5 deliverable 2.
//!
//! A sweep runs an explicit list of (config, pack, script) combinations
//! through the run scorer, collects one [`ScoreRecord`] per run, and writes
//! a comparison table. Runs execute sequentially; each run is deterministic
//! on its own (same guarantee as `genesis score`), and nothing about the
//! batch — order, neighbors, previous results — can reach into a run, so
//! reordering the spec reorders nothing but the work schedule. The table is
//! sorted by the headline score (name as tiebreak), never by batch order.

use genesis_config::{ActionScript, ConfigError, RulePack, SimConfig};
use genesis_observer::{ObserverConfig, RunScore, ScoreRecord, StructureTracker, Timeline};
use genesis_sim::Simulation;
use genesis_sim::interact::RuleSet;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One run in a sweep: a name (the record's filename and table row) plus
/// the same inputs `genesis score` takes. `ticks`/`every` fall back to the
/// sweep-level defaults when omitted.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSpec {
    pub name: String,
    #[serde(default)]
    pub config: Option<PathBuf>,
    #[serde(default)]
    pub rules: Option<PathBuf>,
    #[serde(default)]
    pub actions: Option<PathBuf>,
    #[serde(default)]
    pub ticks: Option<u64>,
    #[serde(default)]
    pub every: Option<u64>,
}

/// A sweep: shared defaults plus the explicit run list. RON on disk.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepSpec {
    /// Default tick count for runs that don't override it.
    pub ticks: u64,
    /// Default observer sample cadence, in ticks.
    pub every: u64,
    /// Observer config applied to every run; defaults when omitted.
    /// Observer config changes what is reported, never what happens — one
    /// sweep uses one observer so its scores are comparable.
    #[serde(default)]
    pub observer: Option<PathBuf>,
    pub runs: Vec<RunSpec>,
}

impl SweepSpec {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let spec: SweepSpec =
            ron::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.runs.is_empty() {
            return Err(ConfigError::Invalid("sweep has no runs".into()));
        }
        if self.ticks == 0 || self.runs.iter().any(|r| r.ticks == Some(0)) {
            return Err(ConfigError::Invalid(
                "sweep ticks must be at least 1".into(),
            ));
        }
        if self.every == 0 || self.runs.iter().any(|r| r.every == Some(0)) {
            return Err(ConfigError::Invalid(
                "sweep sample cadence (every) must be at least 1 tick".into(),
            ));
        }
        let mut names: Vec<&str> = self.runs.iter().map(|r| r.name.as_str()).collect();
        names.sort_unstable();
        if let Some(w) = names.windows(2).find(|w| w[0] == w[1]) {
            return Err(ConfigError::Invalid(format!(
                "duplicate run name {:?}: names key records and table rows",
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
                    "run name {name:?} must be non-empty and use only \
                     [A-Za-z0-9._-]: it becomes a filename"
                )));
            }
        }
        Ok(())
    }
}

/// Run one simulation and score it — the one code path behind both
/// `genesis score` and every sweep run. Inputs are paths so the record can
/// stamp them as given.
pub fn score_run(
    config: &Option<PathBuf>,
    rules: &Option<PathBuf>,
    actions: &Option<PathBuf>,
    ticks: u64,
    every: u64,
    observer_config: ObserverConfig,
) -> Result<ScoreRecord, Box<dyn std::error::Error>> {
    Ok(score_run_capped(config, rules, actions, ticks, every, observer_config, None)?.0)
}

/// [`score_run`] with the search loop's deterministic circuit breaker: when
/// `bond_cap` is set and an observer sample counts more bonds than the cap,
/// the run stops at that sample — the blow-up sample itself is recorded (so
/// the record shows what tripped the breaker) and the returned flag is true.
/// The record stamps the ticks *actually simulated*, so a capped record is
/// still exactly reproducible by `genesis score --ticks <record.ticks>`.
/// The cap reads simulated state at deterministic tick boundaries — never
/// wall time — so whether it fires is a pure function of the run, and a
/// search that uses it stays replayable on any machine (same build).
#[allow(clippy::too_many_arguments)]
pub fn score_run_capped(
    config: &Option<PathBuf>,
    rules: &Option<PathBuf>,
    actions: &Option<PathBuf>,
    ticks: u64,
    every: u64,
    observer_config: ObserverConfig,
    bond_cap: Option<usize>,
) -> Result<(ScoreRecord, bool), Box<dyn std::error::Error>> {
    let sim_config = match config {
        Some(p) => SimConfig::load(p)?,
        None => SimConfig::default(),
    };
    let rule_set = match rules {
        Some(p) => RuleSet::compile(&RulePack::load(p)?),
        None => RuleSet::default(),
    };
    let script = match actions {
        Some(p) => ActionScript::load(p)?,
        None => ActionScript::default(),
    };

    let seed = sim_config.seed;
    let mut sim = Simulation::with_rules_and_actions(&sim_config, rule_set, script);
    let mut tracker = StructureTracker::new(observer_config);
    let mut history = Timeline::new(observer_config);

    let mut simulated = 0;
    let mut capped = false;
    for i in 1..=ticks {
        sim.tick();
        simulated = i;
        if i % every == 0 {
            let snap = sim.snapshot();
            let comps = genesis_observer::bond_components(&snap);
            let stats = genesis_observer::sample_stats(&snap, &comps);
            let bonds = stats.bonds;
            tracker.observe(&comps);
            let metrics = genesis_observer::structure_metrics(&snap, &tracker);
            history.record(stats, metrics);
            if let Some(cap) = bond_cap
                && bonds > cap
            {
                capped = true;
                break;
            }
        }
    }

    let path_string = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string());
    Ok((
        ScoreRecord {
            seed,
            ticks: simulated,
            sample_every: every,
            state_hash: sim.state_hash(),
            config: path_string(config),
            rules: path_string(rules),
            actions: path_string(actions),
            score: RunScore::from_timeline(&history),
        },
        capped,
    ))
}

/// Render the comparison table as markdown. Rows are sorted by the headline
/// score descending, name ascending as the tiebreak — the input order (the
/// batch order) never shows through.
pub fn comparison_table(results: &[(String, ScoreRecord)]) -> String {
    let mut rows: Vec<&(String, ScoreRecord)> = results.iter().collect();
    rows.sort_by(|a, b| {
        b.1.score
            .persistence_complexity
            .total_cmp(&a.1.score.persistence_complexity)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut out = String::new();
    out.push_str(
        "| run | score (pers x cplx) | bounded | fitness | structures fin/peak | largest fin/peak \
         | lifetime fin/peak | cplx peak | info final | self-maint | growing \
         | ticks | state hash |\n",
    );
    out.push_str("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n");
    for (name, r) in rows {
        let s = &r.score;
        // "bounded" = the headline over non-condensed rows only
        // (Q-2026-07-15-A measurement); "—" for records predating the field.
        let bounded = match s.persistence_complexity_bounded {
            Some(v) => format!("{v:.2}"),
            None => "—".to_string(),
        };
        out.push_str(&format!(
            "| {} | {:.2} | {} | {:.2} | {}/{} | {}/{} | {}/{} | {:.2} | {:.2} | {} ({:.2}) \
             | {} ({:.2}) | {} | {:#018x} |\n",
            name,
            s.persistence_complexity,
            bounded,
            crate::search::fitness(s),
            s.structures_final,
            s.structures_peak,
            s.largest_size_final,
            s.largest_size_peak,
            s.lifetime_final,
            s.lifetime_peak,
            s.complexity_peak,
            s.information_final,
            s.self_maintaining_structures,
            s.self_maintaining_peak_confidence,
            s.growing_structures,
            s.growing_peak_confidence,
            r.ticks,
            r.state_hash,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(score: f64, hash: u64) -> ScoreRecord {
        ScoreRecord {
            seed: 0,
            ticks: 100,
            sample_every: 10,
            state_hash: hash,
            config: None,
            rules: None,
            actions: None,
            score: RunScore {
                persistence_complexity: score,
                ..RunScore::zero()
            },
        }
    }

    #[test]
    fn table_sorts_by_score_not_batch_order() {
        let a = vec![
            ("low".to_string(), record(1.0, 1)),
            ("high".to_string(), record(9.0, 2)),
            ("mid".to_string(), record(5.0, 3)),
        ];
        let mut b = a.clone();
        b.reverse();
        let table = comparison_table(&a);
        assert_eq!(table, comparison_table(&b), "batch order must not matter");
        let high = table.find("| high |").unwrap();
        let mid = table.find("| mid |").unwrap();
        let low = table.find("| low |").unwrap();
        assert!(high < mid && mid < low, "descending by headline score");
    }

    #[test]
    fn table_ties_break_by_name() {
        let results = vec![
            ("zeta".to_string(), record(2.0, 1)),
            ("alpha".to_string(), record(2.0, 2)),
        ];
        let table = comparison_table(&results);
        assert!(table.find("| alpha |").unwrap() < table.find("| zeta |").unwrap());
    }

    fn spec(runs: Vec<RunSpec>) -> SweepSpec {
        SweepSpec {
            ticks: 100,
            every: 10,
            observer: None,
            runs,
        }
    }

    fn named(name: &str) -> RunSpec {
        RunSpec {
            name: name.to_string(),
            config: None,
            rules: None,
            actions: None,
            ticks: None,
            every: None,
        }
    }

    #[test]
    fn spec_rejects_empty_duplicate_and_unsafe_names() {
        assert!(spec(vec![]).validate().is_err(), "no runs");
        assert!(
            spec(vec![named("a"), named("a")]).validate().is_err(),
            "duplicate names"
        );
        assert!(spec(vec![named("")]).validate().is_err(), "empty name");
        assert!(
            spec(vec![named("has space")]).validate().is_err(),
            "names become filenames"
        );
        assert!(spec(vec![named("ok-run_1.x")]).validate().is_ok());
    }

    #[test]
    fn spec_rejects_zero_ticks_and_cadence() {
        let mut s = spec(vec![named("a")]);
        s.ticks = 0;
        assert!(s.validate().is_err());
        let mut s = spec(vec![named("a")]);
        s.every = 0;
        assert!(s.validate().is_err());
        let mut s = spec(vec![named("a")]);
        s.runs[0].every = Some(0);
        assert!(s.validate().is_err(), "per-run override is validated too");
    }

    /// The full driver path on a tiny world: two runs, records written,
    /// table written, and the same spec re-run scores identically.
    #[test]
    fn tiny_sweep_is_deterministic_end_to_end() {
        let a = score_run(&None, &None, &None, 50, 10, ObserverConfig::default()).unwrap();
        let b = score_run(&None, &None, &None, 50, 10, ObserverConfig::default()).unwrap();
        assert_eq!(a, b, "same inputs, same record");
        assert_eq!(a.score.samples, 5);
    }
}
