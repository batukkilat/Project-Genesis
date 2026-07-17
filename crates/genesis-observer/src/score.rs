//! Run scoring — the Phase 6.5 experiment loop's unit of comparison.
//!
//! A [`RunScore`] collapses a whole observer [`Timeline`] into one flat,
//! machine-readable record: structure counts, sizes, lifetimes, complexity,
//! information retention, and hypothesis confidences, each as a final value
//! and a run peak. A [`ScoreRecord`] wraps it with the identity stamp (seed,
//! ticks, cadence, final state hash) that makes the run reproducible.
//!
//! Scores are Observer output: they can never enter replay identity, and
//! computing them cannot change a simulated bit. Everything here is a
//! deterministic function of the timeline, which is itself deterministic
//! for a given run and cadence — so the same run always scores identically.

use crate::{HypothesisKind, Timeline};
use genesis_config::ConfigError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

/// The condensation mark: mean bond degree above which a structure (or a
/// whole world, in the search's per-run flag) is read as welded rather than
/// organized (2026-07-13 baseline sweep, finding 1). One definition shared
/// by every consumer — the search leaderboard flag and the bounded headline
/// below — so "condensed" always means the same number. A diagnostic
/// threshold for reporting, never a penalty and never replay identity.
pub const CONDENSED_MEAN_DEGREE: f64 = 50.0;

/// Aggregated Observer metrics for one run. All zeros when the timeline is
/// empty. "Final" fields read the last sample; "peak" fields take the
/// maximum over every sample, so a structure that thrived and died before
/// the end still registers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RunScore {
    /// Samples aggregated.
    pub samples: usize,
    /// Tick of the last sample.
    pub last_tick: u64,
    /// Particles alive at the last sample (emit/absorb move this).
    pub particles_final: usize,
    /// Bonds at the last sample.
    pub bonds_final: usize,
    /// Tracked structures (bonded components) at the last sample.
    pub structures_final: usize,
    /// Most structures alive in any one sample.
    pub structures_peak: usize,
    /// Largest structure at the last sample, in members.
    pub largest_size_final: usize,
    /// Largest structure in any sample.
    pub largest_size_peak: usize,
    /// Longest lifetime among structures alive at the last sample, in
    /// consecutive samples (the observer's persistence metric).
    pub lifetime_final: u32,
    /// Longest lifetime any structure reached at any sample.
    pub lifetime_peak: u32,
    /// Mean lifetime over structures alive at the last sample.
    pub lifetime_final_mean: f64,
    /// Mean complexity over structures alive at the last sample.
    pub complexity_final_mean: f64,
    /// Highest per-structure complexity in any sample.
    pub complexity_peak: f64,
    /// Information held by structure members at the last sample, summed
    /// over structures (the retention metric — signal inside structure,
    /// not loose in the world).
    pub information_final: f64,
    /// Highest per-sample structure-held information total.
    pub information_peak: f64,
    /// Distinct structures ever flagged "possibly self-maintaining".
    pub self_maintaining_structures: usize,
    /// Highest self-maintaining confidence in any sample.
    pub self_maintaining_peak_confidence: f64,
    /// Distinct structures ever flagged "possibly growing".
    pub growing_structures: usize,
    /// Highest growing confidence in any sample.
    pub growing_peak_confidence: f64,
    /// The headline scalar for the Phase 6.5 exit criterion: the maximum of
    /// `persistence × complexity` over every (structure, sample) pair —
    /// long-lived *and* structured beats either alone.
    pub persistence_complexity: f64,
    /// Late bond growth: bonds at the last sample over bonds at the
    /// two-thirds sample (denominator floored at 1). Three searches in a
    /// row the screen's top rank detonated just past the screen horizon;
    /// accelerating bond growth in the final third is that signature,
    /// visible before the detonation. Report-only — never fitness, never
    /// selection. Neutral 1.0 for empty timelines and for records written
    /// before the field existed (serde default).
    #[serde(default = "neutral_growth")]
    pub bonds_growth_late: f64,
    /// The headline scalar restricted to structures respecting the
    /// condensation mark: the maximum of `persistence × complexity` over
    /// every (structure, sample) row with `mean_degree ≤`
    /// [`CONDENSED_MEAN_DEGREE`]. The measurement half of Q-2026-07-15-A
    /// option 2 — the raw scalar keeps its meaning and history; this field
    /// reports what a regime scores *without* welding, so the two regime
    /// classes the phase judges differently can finally be told apart on
    /// numbers. Report-only: never fitness, never selection, never replay
    /// identity, and the exit criterion itself stays scored on the raw
    /// scalar until the parked owner decision says otherwise. `Some(0.0)`
    /// when measured and no row qualified; `None` for records written
    /// before the field existed (a maximum has no neutral sentinel, so
    /// absence must stay distinguishable from zero).
    #[serde(default)]
    pub persistence_complexity_bounded: Option<f64>,
}

fn neutral_growth() -> f64 {
    1.0
}

impl RunScore {
    /// All-zero score: what an empty timeline (nothing ever observed) earns.
    pub fn zero() -> Self {
        RunScore {
            samples: 0,
            last_tick: 0,
            particles_final: 0,
            bonds_final: 0,
            structures_final: 0,
            structures_peak: 0,
            largest_size_final: 0,
            largest_size_peak: 0,
            lifetime_final: 0,
            lifetime_peak: 0,
            lifetime_final_mean: 0.0,
            complexity_final_mean: 0.0,
            complexity_peak: 0.0,
            information_final: 0.0,
            information_peak: 0.0,
            self_maintaining_structures: 0,
            self_maintaining_peak_confidence: 0.0,
            growing_structures: 0,
            growing_peak_confidence: 0.0,
            persistence_complexity: 0.0,
            bonds_growth_late: neutral_growth(),
            persistence_complexity_bounded: Some(0.0),
        }
    }

    /// Aggregate a timeline. Deterministic: iterates samples in recorded
    /// order and structures in their canonical per-sample order.
    pub fn from_timeline(timeline: &Timeline) -> Self {
        let samples = timeline.samples();
        let Some(last) = samples.last() else {
            return RunScore::zero();
        };

        let mut score = RunScore {
            samples: samples.len(),
            last_tick: last.tick,
            particles_final: last.stats.particles,
            bonds_final: last.stats.bonds,
            structures_final: last.structures.len(),
            largest_size_final: last.stats.largest_component,
            ..RunScore::zero()
        };
        // Two-thirds by sample *index*: with a fixed cadence this is
        // two-thirds of the observed horizon.
        let base = samples[(2 * (samples.len() - 1)) / 3].stats.bonds;
        score.bonds_growth_late = last.stats.bonds as f64 / base.max(1) as f64;

        let final_n = last.structures.len();
        if final_n > 0 {
            score.lifetime_final = last.structures.iter().map(|m| m.persistence).max().unwrap();
            score.lifetime_final_mean = last
                .structures
                .iter()
                .map(|m| m.persistence as f64)
                .sum::<f64>()
                / final_n as f64;
            score.complexity_final_mean =
                last.structures.iter().map(|m| m.complexity).sum::<f64>() / final_n as f64;
            score.information_final = last.structures.iter().map(|m| m.information).sum();
        }

        let mut self_maintaining: BTreeSet<u64> = BTreeSet::new();
        let mut growing: BTreeSet<u64> = BTreeSet::new();
        let mut bounded = 0.0f64;
        for s in samples {
            score.structures_peak = score.structures_peak.max(s.structures.len());
            score.largest_size_peak = score.largest_size_peak.max(s.stats.largest_component);
            let mut information = 0.0f64;
            for m in &s.structures {
                score.lifetime_peak = score.lifetime_peak.max(m.persistence);
                score.complexity_peak = score.complexity_peak.max(m.complexity);
                let headline = m.persistence as f64 * m.complexity;
                score.persistence_complexity = score.persistence_complexity.max(headline);
                if m.mean_degree <= CONDENSED_MEAN_DEGREE {
                    bounded = bounded.max(headline);
                }
                information += m.information;
            }
            score.information_peak = score.information_peak.max(information);
            for h in &s.hypotheses {
                match h.kind {
                    HypothesisKind::PossiblySelfMaintaining => {
                        self_maintaining.insert(h.structure);
                        score.self_maintaining_peak_confidence =
                            score.self_maintaining_peak_confidence.max(h.confidence);
                    }
                    HypothesisKind::PossiblyGrowing => {
                        growing.insert(h.structure);
                        score.growing_peak_confidence =
                            score.growing_peak_confidence.max(h.confidence);
                    }
                }
            }
        }
        score.self_maintaining_structures = self_maintaining.len();
        score.growing_structures = growing.len();
        score.persistence_complexity_bounded = Some(bounded);
        score
    }
}

/// One scored run: the score plus the identity stamp that reproduces it.
/// This is the record the sweep driver collects and compares.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreRecord {
    /// Master seed the run derived every RNG stream from.
    pub seed: u64,
    /// Ticks simulated.
    pub ticks: u64,
    /// Observer sample cadence, in ticks.
    pub sample_every: u64,
    /// Final simulation state hash — the replay fingerprint. Re-running the
    /// same build with the same seed, config, pack, and script must
    /// reproduce this exactly; a differing hash means the record describes
    /// a different universe.
    pub state_hash: u64,
    /// Config / rule-pack / action-script paths as given, `None` for
    /// defaults. Documentation for humans reading the record (the
    /// labels-above-the-engine rule): the run's identity is the *contents*
    /// behind them, already fingerprinted by `state_hash`.
    pub config: Option<String>,
    pub rules: Option<String>,
    pub actions: Option<String>,
    pub score: RunScore,
}

impl ScoreRecord {
    /// Pretty RON — the on-disk format sweep results are collected in.
    pub fn to_ron(&self) -> Result<String, ConfigError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| ConfigError::Parse(e.to_string()))
    }

    pub fn from_ron(text: &str) -> Result<Self, ConfigError> {
        ron::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        Self::from_ron(&std::fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObserverConfig, SampleStats, StructureMetrics, Timeline};

    fn stats(tick: u64, particles: usize, bonds: usize, largest: usize) -> SampleStats {
        SampleStats {
            tick,
            particles,
            bonds,
            components: 0,
            largest_component: largest,
            in_multi: 0,
            total_matter: 0.0,
            total_energy: 0.0,
            total_information: 0.0,
        }
    }

    fn row(
        id: u64,
        size: usize,
        persistence: u32,
        complexity: f64,
        information: f64,
    ) -> StructureMetrics {
        StructureMetrics {
            id,
            size,
            persistence,
            stability: 1.0,
            complexity,
            degree_entropy: 0.0,
            mean_degree: 0.0,
            information,
        }
    }

    fn churny_row(id: u64, size: usize, persistence: u32) -> StructureMetrics {
        StructureMetrics {
            // Stability below the 0.75 self-maintaining default: growth is
            // still recordable (growing ignores stability), self-maintaining
            // is not.
            stability: 0.5,
            ..row(id, size, persistence, 1.0, 0.0)
        }
    }

    #[test]
    fn empty_timeline_scores_zero() {
        let tl = Timeline::new(ObserverConfig::default());
        assert_eq!(RunScore::from_timeline(&tl), RunScore::zero());
    }

    #[test]
    fn finals_read_the_last_sample_peaks_read_the_run() {
        let mut tl = Timeline::new(ObserverConfig::default());
        // Sample 1: two structures, one big and complex.
        tl.record(
            stats(100, 50, 20, 8),
            vec![row(1, 8, 3, 4.0, 2.0), row(2, 3, 1, 1.0, 0.5)],
        );
        // Sample 2: the big one died; the survivor aged.
        tl.record(stats(200, 48, 10, 4), vec![row(2, 4, 2, 1.5, 0.25)]);

        let s = RunScore::from_timeline(&tl);
        assert_eq!(s.samples, 2);
        assert_eq!(s.last_tick, 200);
        assert_eq!(s.particles_final, 48);
        assert_eq!(s.bonds_final, 10);
        assert_eq!(s.structures_final, 1);
        assert_eq!(s.structures_peak, 2);
        assert_eq!(s.largest_size_final, 4);
        assert_eq!(s.largest_size_peak, 8);
        assert_eq!(s.lifetime_final, 2);
        assert_eq!(s.lifetime_peak, 3, "the dead structure's tenure counts");
        assert_eq!(s.lifetime_final_mean, 2.0);
        assert_eq!(s.complexity_final_mean, 1.5);
        assert_eq!(s.complexity_peak, 4.0);
        assert_eq!(s.information_final, 0.25);
        assert_eq!(s.information_peak, 2.5, "sample-1 total across structures");
        assert_eq!(
            s.persistence_complexity, 12.0,
            "structure 1 at sample 1: 3 × 4.0"
        );
        assert_eq!(
            s.persistence_complexity_bounded,
            Some(12.0),
            "every row here has mean degree 0: bounded equals the raw scalar"
        );
        assert_eq!(
            s.bonds_growth_late, 0.5,
            "two samples: base is sample 0 (20 bonds), final is 10"
        );
    }

    #[test]
    fn bounded_headline_excludes_condensed_rows_only() {
        let dense = |id, persistence, complexity, mean_degree| StructureMetrics {
            mean_degree,
            ..row(id, 100, persistence, complexity, 0.0)
        };
        let mut tl = Timeline::new(ObserverConfig::default());
        // One welded blob carrying the top raw headline, one structure
        // exactly at the mark (a boundary the mark must include — `condensed`
        // means *above* the threshold), one sparse structure below it.
        tl.record(
            stats(100, 300, 5000, 100),
            vec![
                dense(1, 10, 9.0, 80.0),
                dense(2, 8, 5.0, CONDENSED_MEAN_DEGREE),
                dense(3, 6, 4.0, 2.0),
            ],
        );
        let s = RunScore::from_timeline(&tl);
        assert_eq!(s.persistence_complexity, 90.0, "raw scalar keeps the blob");
        assert_eq!(
            s.persistence_complexity_bounded,
            Some(40.0),
            "bounded takes the at-mark row (8 × 5.0), never the blob"
        );

        // A timeline where *every* structure is welded: measured, nothing
        // qualified — Some(0.0), which is distinguishable from an old
        // record's None.
        let mut tl = Timeline::new(ObserverConfig::default());
        tl.record(stats(100, 300, 5000, 100), vec![dense(1, 10, 9.0, 80.0)]);
        let s = RunScore::from_timeline(&tl);
        assert_eq!(s.persistence_complexity_bounded, Some(0.0));
    }

    #[test]
    fn bonds_growth_reads_the_final_third() {
        let mut tl = Timeline::new(ObserverConfig::default());
        for (i, bonds) in [10, 40, 100, 1000].iter().enumerate() {
            tl.record(stats((i as u64 + 1) * 100, 50, *bonds, 0), vec![]);
        }
        // Four samples: base index (2·3)/3 = 2 → 100 bonds; 1000/100.
        let s = RunScore::from_timeline(&tl);
        assert_eq!(s.bonds_growth_late, 10.0);

        // Zero bonds at the base: denominator floors at 1 instead of
        // dividing by zero.
        let mut tl = Timeline::new(ObserverConfig::default());
        for (i, bonds) in [0, 0, 7].iter().enumerate() {
            tl.record(stats((i as u64 + 1) * 100, 50, *bonds, 0), vec![]);
        }
        let s = RunScore::from_timeline(&tl);
        assert_eq!(s.bonds_growth_late, 7.0);
    }

    #[test]
    fn records_without_the_growth_field_load_neutral() {
        // A pre-v1.2 record: serialize a current one, strip the fields
        // added since, and reparse — committed search-01..04 artifacts must
        // keep loading. Growth loads as the neutral 1.0; the bounded
        // headline loads as None ("not measured"), never as a fake zero.
        let mut tl = Timeline::new(ObserverConfig::default());
        tl.record(stats(10, 5, 2, 3), vec![row(1, 3, 1, 1.7, 0.4)]);
        let record = ScoreRecord {
            seed: 42,
            ticks: 1000,
            sample_every: 10,
            state_hash: 1,
            config: None,
            rules: None,
            actions: None,
            score: RunScore::from_timeline(&tl),
        };
        let text = record.to_ron().unwrap();
        let stripped: String = text
            .lines()
            .filter(|l| {
                !l.contains("bonds_growth_late") && !l.contains("persistence_complexity_bounded")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_ne!(text, stripped, "the fields must actually serialize");
        let loaded = ScoreRecord::from_ron(&stripped).unwrap();
        assert_eq!(loaded.score.bonds_growth_late, 1.0);
        assert_eq!(loaded.score.persistence_complexity_bounded, None);
        assert_eq!(loaded.score.bonds_final, record.score.bonds_final);
    }

    #[test]
    fn hypotheses_count_distinct_structures_and_peak_confidence() {
        // Timeline::record derives hypotheses itself, so drive real ones:
        // one structure old + stable (self-maintaining), one growing.
        let config = ObserverConfig {
            window: 3,
            self_maintaining_age: 4,
            ..ObserverConfig::default()
        };
        let mut tl = Timeline::new(config);
        for i in 1..=6u32 {
            tl.record(
                stats(i as u64, 10, 5, 4),
                vec![
                    // Structure 1: fixed size, stable, ages past the
                    // threshold — self-maintaining only (no growth).
                    row(1, 4, i, 2.0, 1.0),
                    // Structure 2: strictly growing every sample but too
                    // churny for self-maintaining.
                    churny_row(2, i as usize + 1, i),
                ],
            );
        }
        let s = RunScore::from_timeline(&tl);
        assert_eq!(s.self_maintaining_structures, 1);
        assert_eq!(s.growing_structures, 1);
        // Age ramp at persistence 6 of 2*4: 0.75, stability 1.0.
        assert_eq!(s.self_maintaining_peak_confidence, 0.75);
        assert_eq!(s.growing_peak_confidence, 1.0);
    }

    #[test]
    fn score_record_roundtrips_through_ron() {
        let mut tl = Timeline::new(ObserverConfig::default());
        tl.record(stats(10, 5, 2, 3), vec![row(1, 3, 1, 1.7, 0.4)]);
        let record = ScoreRecord {
            seed: 42,
            ticks: 1000,
            sample_every: 10,
            state_hash: 0xDEAD_BEEF_0BAD_F00D,
            config: Some("configs/env-gradient.ron".into()),
            rules: Some("packs/chains.ron".into()),
            actions: None,
            score: RunScore::from_timeline(&tl),
        };
        let text = record.to_ron().unwrap();
        assert_eq!(ScoreRecord::from_ron(&text).unwrap(), record);
    }
}
