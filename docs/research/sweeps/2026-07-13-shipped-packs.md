# Sweep 2026-07-13 — the shipped-content baseline

The first Phase 6.5 sweep: every shipped pack with its canonical pairing,
scored by the run scorer (Q-2026-07-13-A) via the sweep driver
(Q-2026-07-13-B). This is the corpus every discovered regime must beat on
`persistence_complexity`; negative results below are recorded so dead
regions are not re-searched.

Reproduce with:

```sh
genesis sweep --spec sweeps/shipped-packs.ron --out <dir>
```

Machine: 4-core Intel Xeon @ 2.80GHz (cloud box), build `20a4efa`
(rustc 1.96.1, release profile). 20 000 ticks, observer sampled every 100
ticks (200 samples), default `ObserverConfig`. Raw records live next to
this doc in `2026-07-13-shipped-packs/`.

## Results

Sorted by the headline score, `max persistence × complexity` over every
(structure, sample) pair. `fin/peak` = last-sample value / run maximum.

| run | score | structures fin/peak | largest fin/peak | lifetime fin/peak | cplx peak | info-in-structs final | self-maint | growing | wall time |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| actual | 3631.36 | 2/283 | 4916/4921 | 172/172 | 21.12 | 0.00 | 276 | 216 | 1302.8s |
| bands | 2711.93 | 1/412 | 6904/6904 | 122/122 | 22.23 | 3407.63 | 322 | 418 | 6451.1s |
| sandbox | 2178.48 | 65/590 | 9035/10459 | 146/146 | 15.90 | 0.00 | 134 | 572 | 556.2s |
| sieve | *20k-tick run in progress — row + record land in a follow-up commit* | | | | | | | | |
| full-stack | 1806.26 | 683/686 | 57/192 | 200/200 | 13.39 | 2863.75 | 1209 | 1594 | 153.7s |
| chains | 1196.38 | 852/868 | 15/15 | 200/200 | 6.99 | 0.00 | 1849 | 1794 | 53.5s |
| echoes | 0.00 | 0/0 | 0/0 | 0/0 | 0.00 | 0.00 | 0 | 0 | — (re-run) |
| churn | 0.00 | 0/0 | 0/0 | 0/0 | 0.00 | 0.00 | 0 | 0 | 34.8s |
| diffusion | 0.00 | 0/0 | 0/0 | 0/0 | 0.00 | 0.00 | 0 | 0 | 35.1s |
| hoarders | 0.00 | 0/0 | 0/0 | 0/0 | 0.00 | 0.00 | 0 | 0 | 37.3s |
| physics-only | 0.00 | 0/0 | 0/0 | 0/0 | 0.00 | 0.00 | 0 | 0 | 25.0s |

Two rows differ from the batch the driver ran (wall times above are from
that batch; state hashes and scores in the committed records):

- **echoes** was originally paired with the default config — whose
  `initial.information` range is `0..0` — so its imprint rule (gated on
  information ≥ 0.5) could never fire: a **null run**, not a measurement.
  The spec now pairs it with `configs/env-gradient.ron` (information
  0..1); the committed record is that re-run. Its score is still zero,
  but for a different reason (see finding 2).
- **sieve** landed after the batch started; its record was produced by
  `genesis score` with identical parameters (same build, same cadence —
  the driver calls the same `score_run` function, so the records are
  interchangeable).

## Findings

### 1. The score is a condensation contest right now

The top three scores are worlds that condensed into one or two giant
blobs: `actual` ends with 4 916 of ~10 000 particles in one component,
`bands` with 6 904, `sandbox` with 9 035 at peak. Their complexity comes
from `ln(size) + degree-entropy + ln(1 + mean_degree)` — and their mean
degrees are enormous, because **bonds accumulate monotonically in these
packs**: `actual` ends at 1.08M bonds, `bands` at 2.09M (mean degree
~200-400; their break rules fire too rarely to matter). Nothing in
`persistence × complexity` penalizes "the whole world welded into one
lump that never changes again". A discovered regime can therefore "win"
by condensing faster — which is exactly the plateau ("blobs") Phase 6.5
exists to escape, sitting at the top of our own leaderboard.

**Consequence for the search deliverable:** the fitness function needs
more than the headline scalar — at minimum something that rewards *many
distinct* persistent structures (structures_final), membership turnover
survived (stability < 1 with identity kept), or information retention,
before the evolutionary loop is switched on. Optimizing the current
scalar alone will breed condensation. That fitness choice is a design
fork to log when search lands, with this sweep as its evidence.

### 2. Four shipped regimes are invisible to the metric — by design, worth stating

`diffusion`, `hoarders`, `churn`, and (fixed) `echoes` score exactly
zero: they form no bonds, and every v1 Observer metric — structures,
persistence, complexity, even information retention — is defined over
**bond-graph components**. These packs have real dynamics (hoards,
population balance, information fronts), but their order lives in
quantity *distributions*, which the v1 Observer does not measure. This is
the honest boundary of the current instrument, not a bug: the Phase 6.5
exit criterion is about persistent structure, and "structure" currently
means bonds. If a future phase wants to score field-like or
distributional order (echoes fronts, hoard spatial clustering), that is a
new Observer metric family — an open question for the search phase, not a
reason to bend the score definition mid-experiment.

Practical corollary: **a pack that wants to register on the leaderboard
must express its order through bonds.** Sieve was authored with exactly
this constraint (its BIND/SHED rules build structure out of maintained
information).

### 3. A pack is not an experiment — a (config, pack, script) triple is

The echoes null run is the lesson: pack semantics depend on the config's
initial ranges and physics (decay), and a mis-pairing produces a
plausible-looking zero. The sweep spec treats the triple as the unit,
which is correct; the corpus now encodes the canonical pairing for every
pack. Any future zero should be checked against "did the rules ever
fire" (population/bond counts in the record make this visible: a null
run has pristine particle count and zero bonds *and* zero transfers'
side effects — compare `physics-only`'s state hash drift).

### 4. LOD makes lifetime partially an artifact of freezing

`full-stack` (the only LOD-enabled row) has structures alive for all 200
samples — its cold chunks tick at reduced rates, so bonded structures in
quiet regions are effectively preserved. Within one sweep this is fine
(every row would gain the same advantage under the same policy), but
**cross-config score comparisons must hold the LOD policy fixed**, or
lifetime becomes a measure of how much of the world is frozen.
`shipped-packs.ron` keeps LOD off everywhere except `full-stack`, which
is in the corpus as a determinism canary, not a fair contestant.

### 5. Dense bonding is the wall-time driver (and will throttle search)

Wall time tracks bond count, not particle count: `bands` (2.09M bonds)
took 108 minutes; `sandbox` (50k bonds at the end, churning population)
took 9; bond-free packs take ~35s. Springs + the per-tick CSR mirror
dominate. For the search loop this matters twice: (a) runaway-bonding
mutants will eat the batch budget — the driver may eventually want a
per-run wall-time or bond-count circuit breaker (a driver feature, never
a sim change; parked until search actually hits it); (b) 20k-tick
evaluations of interesting (= bonded) regimes cost minutes, so the
evolutionary loop's generation size must be planned around that.

### 6. Sieve (20k-tick result pending)

Authored today against finding 2's corollary: its structure is built
from information-gated bonds, so selection pressure and the scored
observable are the same thing. Short-horizon behavior (600 ticks):
population grows through info-gated splits (~13k particles), ~1.4k
structures, high structure-held information. The 20k-tick row and what
it says about long-run selection dynamics land with the follow-up
commit.

## Corpus

| run | config | rules | actions |
|---|---|---|---|
| physics-only | (default) | — | — |
| diffusion | (default) | packs/diffusion.ron | — |
| hoarders | (default) | packs/hoarders.ron | — |
| chains | (default) | packs/chains.ron | — |
| echoes | configs/env-gradient.ron | packs/echoes.ron | — |
| churn | (default) | packs/churn.ron | — |
| actual | (default) | packs/actual.ron | — |
| sandbox | (default) | packs/sandbox.ron | — |
| bands | configs/env-gradient.ron | packs/bands.ron | — |
| sieve | configs/sieve.ron | packs/sieve.ron | — |
| full-stack | configs/full-stack.ron | packs/bands.ron | scripts/full-stack.ron |

Note the density caveat: the default config spawns 10k particles in a
4096² world; env-gradient/sieve/full-stack worlds are 1024² — 16× denser,
so interaction rates are not comparable across those groups. Scores are
comparable as "what this experiment produces", not as "which rule set is
better per encounter".
