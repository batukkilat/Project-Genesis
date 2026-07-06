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

The amplifying regime finds its own dynamic equilibrium — nobody coded one:

- Population boils to 9 687 by tick 750 (fission cheap), then the absorb
  side wins and it settles to 7 451 by tick 3 000; matter and energy stay
  conserved to f32 tolerance through all of it (4407.480 vs 4407.493 at
  spawn, ~3·10⁻⁶ relative, across thousands of create/destroy events).
- A giant bonded component (~4 000 members) plus ~35 smaller structures
  persist: 34 of 36 components are ≥ 5 samples old by the end.
- The runaway info economy drove the information total from ~4·10³ to
  ~4·10⁸ by tick 750 and to **NaN** by tick 2 000 (f32 overflow → inf →
  inf−inf). The simulation stays deterministic through it; conditions on
  NaN simply stop firing. Recorded as QUESTIONS.md Q-2026-07-06-B
  (quantity overflow policy). **Resolved 2026-07-06:** information is now
  clamped to `information_max` (default 1e30) at interaction commit — save
  format v7, decisions log Q-2026-07-06-B. Amplifying packs saturate
  instead of reaching NaN, so the sandbox hashes below are pre-cap
  historical values; re-running sandbox under a current build gives a
  different (still deterministic) hash.
- Final state hash `0x2307b9e1a2f7b35b`.

### Determinism

`genesis verify` (two fresh runs + save/resume + single-thread, all four
hashes compared) against the review config:

| Pack | Ticks | Result | Hash |
|---|---|---|---|
| chains.ron | 2 000 | DETERMINISTIC | `0xdf76a421db16699a` |
| actual.ron | 600 | DETERMINISTIC | `0xdf2f7e17dcc04bd9` |
| sandbox.ron | 600 | DETERMINISTIC | `0xcf2d97116959dd94` |
| sandbox.ron | 2 500 (crosses the NaN tick) | DETERMINISTIC | `0x631251cb73fa602a` |

Conservation is asserted continuously by the test suite
(`interactions_conserve_totals`, `create_destroy_conserves_and_stays_deterministic`,
`matter_is_conserved_exactly`) and visible in every report line above.

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

**Phase 3 exit criteria pass.**

- *A nontrivial rule pack produces persistent multi-particle structures
  nobody explicitly coded*: chains.ron authors two pairwise rules and gets
  ~160 long-lived multi-particle structures, one of which keeps its
  identity for 19 500 ticks; sandbox.ron gets a population equilibrium and
  a persistent giant component from six rules about flows. No structure,
  size, count, or lifetime appears anywhere in the content.
- *Determinism still holds*: every pack verifies hash-identical across
  fresh runs, save/resume, and thread counts (chains at 2 000 ticks,
  actual and sandbox at 600) — and the 2 500-tick sandbox verify proves
  the property survives even the f32 info overflow into NaN.
- *Conservation still holds*: matter and energy totals constant (exact in
  the pure-bonding run, ≤ 3·10⁻⁶ relative under heavy create/destroy
  churn), asserted continuously by the test suite.

Phase 3 is done. Phase 4's first item (adaptive-detail groundwork) is
blocked on QUESTIONS.md Q-2026-07-06-A; Q-2026-07-06-B (overflow policy)
is open but non-blocking.
