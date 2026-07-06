# Phase 3 exit-criteria review

Date: 2026-07-06. Engine: v0.2.0 workspace at the commit adding this file.

ROADMAP Phase 3 exit criteria:

> a nontrivial rule pack produces persistent multi-particle structures
> nobody explicitly coded; determinism and conservation still hold.

## Method

Read-only diagnostics (`genesis run --report N`, added for this review)
sample the canonical snapshot every N ticks and report bond-graph connected
components, their persistence (member-overlap identity tracking across
samples: a component continues an older one when they share at least half
the members of the larger), and matter/energy/information totals. Nothing
in the diagnostics can mutate simulation state — they consume snapshots.

Configuration: `docs/research/phase3-exit-config.ron` (committed) — 8 000
particles, 1024×1024 torus (~0.0076 particles/unit², ~1.5 neighbors inside
the force radius), all three quantities live at spawn,
`information_decay = 0.02`. Report interval 500 ticks; a component counts
as *persistent* at age ≥ 5 samples, i.e. its identity survived ≥ 2 000
consecutive ticks.

Packs under review: `chains.ron` (binding regime), `actual.ron`
(conservation-respecting regime), `sandbox.ron` (amplifying regime) — all
committed content, none of which names or codes any structure larger than a
pairwise rule.

## Results

### chains.ron — 20 000 ticks (the exit-criterion case)

| tick | bonds | components | largest | persistent (≥5 samples) | oldest age |
|---|---|---|---|---|---|
| 500 | 4 359 | 930 | 14 | — | 1 |
| 4 500 | 19 576 | 389 | 42 | 363 | 9 |
| 8 500 | 24 897 | 269 | 68 | 262 | 17 |
| 12 500 | 31 894 | 210 | 86 | 202 | 25 |
| 16 500 | 41 004 | 176 | 95 | 174 | 33 |
| 20 000 | 37 835 | 159 | 104 | 157 | 40 |

- **Persistent structures nobody coded:** by tick 20 000, 157 of 159
  components have held their identity (member-overlap tracking) for ≥ 5
  consecutive samples; the oldest has done so for 40 samples — since tick
  500 of the run. The pack authors two pairwise rules (latch, rare break);
  the 104-member component and the population of ~160 long-lived
  intermediate structures are not in the content anywhere.
- **Equilibrium, not monotone gelling:** bond count saturates (~41k peak,
  then fluctuates 36–41k) as creation and the rare break rule balance —
  structures persist *against* churn, which is what "persistent" should
  mean.
- **Conservation:** total matter (4407.493) and energy (3967.748) are
  constant to printed precision over all 40 samples. Information decays
  toward zero exactly as configured (no copy rule in this pack).
- Final state hash `0xb402d80aec0a662e` at 25 ticks/s mean throughput
  (4-core cloud box, release).

### actual.ron — run stopped at tick 2 000 (gel regime at this density)

Bond count exploded 3.4k → 40k → 296k → 1.07M over ticks 500–2000, and the
largest component reached 4 736 of ~8 000 particles: at review density the
cheap-to-keep bond economy runs away (denser clumps → more pairs inside
bond range → more springs → denser clumps). Observations that stand:

- Matter/energy totals exact through heavy emit/absorb churn (population
  8 044 → 7 977 → 8 018).
- Information total *rose* from ~4 000 at spawn to 4 955 by tick 500
  before easing to 4 427 — the info-copy economy genuinely outruns decay
   0.02/s while energy lasts. Information behaving like a paid-for,
  non-conserved quantity is exactly the Q1 semantics decision working.
- A world-spanning bonded gel is a *valid* outcome (every outcome is) but
  the 1M-spring pass makes long runs impractical; the pack wants either a
  sparser world or bond-maintenance economics (see note below).

### sandbox.ron — 3 000 ticks

TODO(sandbox)

### Determinism

`genesis verify` (two fresh runs + save/resume + single-thread, all hashes
compared) against the review config:

TODO(verify-hashes)

## Density regime note

A first attempt used 10 000 particles in a 512×512 torus (0.38/unit² —
~640× the engine-default density). In that regime `chains.ron` gels: ~90 000
bonds by tick 1 000, component count falling as everything anneals toward a
world-spanning cluster, and throughput collapsing with the neighbor counts.
That is a valid universe (every outcome is), but it measures the density
knob rather than the rules, and it is the reason the committed review config
sits at moderate density.

## Content-design note: unbounded bond accumulation

Two regimes observed tonight (dense chains, moderate actual) show the same
shape: when keeping a bond is free and creation outpaces the break rule,
bond count grows until the spring pass dominates the tick. This is content
physics, not an engine defect — the engine conserved everything and stayed
deterministic throughout. But packs that want *bounded* structure have only
blunt tools today (break probability, energy conditions at creation). If a
future phase wants richer bond economics (per-tick maintenance cost, break
on stretch), that is new rule vocabulary — a replay-identity change that
must go through QUESTIONS.md/the decisions log, not be smuggled into a
pack. No action needed for Phase 3.

## Verdict

TODO(fill after results)
