# Phase 5 exit-criteria review

Date: 2026-07-08. Engine: v0.3.0 workspace at the commit adding this file.

ROADMAP Phase 5 exit criteria:

> Observer flags structures in Phase 3/4 outputs that match what
> developers see by eye; simulation state hash provably unaffected by
> Observer presence.

## What shipped (landing order, docs/research/observer-design.md)

1. `genesis-observer` crate — read-only by construction (no `&mut` to any
   simulation type), consuming `WorldSnapshot`s.
2. Configurable overlap tracking with stable observer-side structure ids
   (`ObserverConfig`, RON, never replay identity; ids never reused).
3. Metrics v1 (Q-2026-07-08-D): persistence, stability (Jaccard of
   consecutive memberships), complexity
   (`ln(size) + degree-entropy + ln(1 + mean_degree)`), information.
4. Hypotheses v1 (Q-2026-07-08-E): *possibly self-maintaining* and
   *possibly growing*, confidence-scored, positives-only; timeline
   recording with RON dump (`genesis run --report N --timeline <path>`).

## Criterion 1 — flags match the eye

### chains.ron (Phase 3 content), default config, 10 000 ticks, sample every 1 000

The binding regime the eye sees: bonds accumulate steadily (354 → 2 053),
components multiply and slowly consolidate (largest 4 → 10), persistent
population grows without bound in this window (242 of 683 components ≥ 5
samples old by tick 10 000).

What the observer flags, with no knowledge of any of that:

- **possibly growing** appears at the first full window (sample 5) and
  tracks the visible growth regime: 22 → 85 flagged structures across
  samples 5–10.
- **possibly self-maintaining** fires exactly at the age threshold
  (sample 10): 18 structures that held identity for all 10 samples with
  windowed stability ≥ 0.75 — the long-lived chains the eye picks out.
- Final hash `0x2dc9276516a7f0a9`, 421 ticks/s (4-core cloud box,
  release; observation cost included).

### bands.ron on configs/env-gradient.ron (Phase 4 content), 3 000 ticks, sample every 300

The env-gated world the eye sees: structures concentrate where the
environment allows (Phase 4's sim test proves the banding), bonds grow
2 019 → 11 836, components consolidate 373 → 195 while the largest grows
14 → 54 members.

Observer output:

- **possibly growing**: 101 structures at the first full window, ~100
  per sample thereafter (624 positive records over the run) — top
  confidence 0.75 (3 of 4 window steps strict). Matches a world whose
  in-multi population rises every single sample (1 321 → 2 711).
- **possibly self-maintaining**: 63 structures at sample 10, including
  observer id 1 — a structure tracked from the very first sample at
  tick 300 through tick 3 000 under member churn.
- Conservation visible in every line: matter/energy/information totals
  constant to printed precision (this pack has no info dynamics).
- Final hash `0x5a94a0e739b4f2e9`, 130 ticks/s with observation.

Both regimes: what the hypothesis layer flags is what the report lines
(and Phase 3/4 reviews) show by eye. No aspirational labels shipped —
life/intelligence/civilization wait for metrics that could honestly move
them.

## Criterion 2 — state hash provably unaffected

- **Type-system fact**: the crate never receives a mutable reference to
  simulation state; observation cannot write.
- **Library-level test**: `observer_on_or_off_identical_state_hashes`
  runs the full pipeline (components, stats, tracker, metrics,
  hypotheses, timeline) every 10 ticks and asserts the final hash equals
  an unobserved run's.
- **CLI-level check (this review)**: the bands run above, re-run without
  `--report`/`--timeline`, produces the identical final hash
  `0x5a94a0e739b4f2e9`.
- Observer determinism: `observer_output_is_deterministic_for_the_same_run`
  asserts two observed passes of the same universe produce identical
  trace *and* identical serialized timeline.

## Verdict

**Phase 5 exit criteria pass.**

Deferred, recorded in the design doc: the *adaptation* metric (needs
environment-correlation history), richer hypotheses (self-replication
needs structure-signature similarity), and zero-copy snapshot machinery
(a Phase 6 renderer problem — open design question 4). None block the
criteria.
