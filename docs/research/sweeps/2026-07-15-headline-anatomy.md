# Headline anatomy — the exit-criterion scalar structurally favors condensation

Answers the sharpest question search-02 left open (finding 5 of
[2026-07-14-search-02.md](2026-07-14-search-02.md)): *can a
non-condensing regime reach `actual`'s 3 631 on persistence ×
complexity, or does the scalar structurally favor condensation?* The
answer is the second, and it is now measured and bounded rather than
suspected. Evidence: a per-term decomposition of the complexity metric
on real structures (instrument commit: `feat(observer): report the
complexity decomposition per structure`), plus a maximum-entropy ceiling
argument over the decomposition.

Machine: 4-core Intel Xeon @ 2.80 GHz (cloud box), release build of
`b02bf71` (rustc 1.96.1). Probe artifacts and the extraction script live
next to this doc in `2026-07-15-headline-anatomy/`; raw timeline dumps
(2–15 MB each) are reproducible from the commands below and not
committed.

## Method

The complexity scalar is `C = ln(size) + H + ln(1 + d̄)` where `H` is the
Shannon entropy (nats) of the member bond-degree histogram and `d̄` the
mean member degree. `StructureMetrics` now reports `degree_entropy` and
`mean_degree` alongside the committed scalar (the decomposition is
tested to reproduce `C` bit-for-bit), so a timeline dump shows *which
term carries* any complexity value. Five regimes were probed at the 3 k
gate horizon and the search-02 champion also at the 20 k corpus horizon:

```sh
genesis run --rules packs/actual.ron            --ticks 3000  --report 100 --timeline actual-3k.tl.ron
genesis run --config configs/env-gradient.ron --rules packs/bands.ron \
                                                --ticks 3000  --report 100 --timeline bands-3k.tl.ron
genesis run --rules packs/sandbox.ron           --ticks 3000  --report 100 --timeline sandbox-3k.tl.ron
genesis run --config configs/sieve.ron --rules packs/sieve.ron \
                                                --ticks 3000  --report 100 --timeline sieve-3k.tl.ron
genesis run --config docs/research/sweeps/2026-07-14-search-02/g006/g006-i009.config.ron \
            --rules  docs/research/sweeps/2026-07-14-search-02/g006/g006-i009.pack.ron \
                                                --ticks 3000  --report 100 --timeline champion-3k.tl.ron
# and the same champion pairing at --ticks 20000
python3 2026-07-15-headline-anatomy/anatomy.py *.tl.ron   # -> probes.txt
```

Cross-validation: every probe's headline reproduces its committed score
record to all printed digits (actual 208.00857…, bands 291.46982…,
sandbox 369.81488…, sieve 318.55212…, champion 253.86747… — against the
2026-07-14 3 k sweep records committed by build `b8291cd`). The
decomposition instrument changed no simulated bit and no committed
scalar, observed across builds once more.

## Finding 1 — three regime classes, three term profiles

Decomposed headline rows at the 3 k gate (P = persistence in samples,
terms in nats):

| regime | headline | = P × C | ln size | H | ln(1+d̄) | class |
|---|---:|---:|---:|---:|---:|---|
| sandbox | 369.8 | 26 × 14.22 | 7.93 (2 778) | 3.56 | 2.74 (d̄ 14.4) | condenser: **all three terms grow together** |
| sieve | 318.6 | 29 × 10.99 | 4.11 (61) | 3.00 | 3.87 (d̄ 47.1) | clique-web: degree term ≈ size term |
| bands | 291.5 | 30 × 9.72 | 3.74 (42) | 2.78 | 3.19 (d̄ 23.4) | condenser (early) |
| champion g006-i009 | 253.9 | 28 × 9.07 | 3.61 (37) | 2.32 | 3.14 (d̄ 22.1) | clique-web |
| actual | 208.0 | 29 × 7.17 | 4.71 (111) | 1.35 | 1.11 (d̄ 2.0) | condenser (late-blooming) |

Trajectories separate the classes. In condensers every term rises
monotonically and entropy *stays* high (sandbox's giant: H 1.26 → 3.13 →
3.56 while size goes 16 → 275 → 2 778) — a condensing giant is
permanently mid-accretion, so its degree distribution stays wide. In
clique-webs the entropy term **collapses as the web completes**: the
champion's headline structure ends the probe at size 37, `d̄` 36.0,
H = 0.000 — a complete graph, every member playing the identical role —
and sieve's headline structure runs H 2.32 → 0.38 → 0.22 as `d̄` climbs
to 62. For a completed clique `C = 2·ln(size)` exactly (the entropy term
is zero and `ln(1+d̄) = ln(size)`), which caps any clique respecting the
condensation mark (`d̄ ≤ 50`, i.e. size ≤ 51) at `C = 2·ln 51 = 7.86` —
headline ≤ 1 573 at full 200-sample persistence. **The champion's own
regime class cannot approach `actual` without blowing past the mark.**

## Finding 2 — the champion is itself past the condensation mark at 20 k

Search-02 described the champion's bond growth as "bounded" — true at
the screen horizon (world mean degree 13.2 at 3 k) but not at the corpus
horizon: 2 × 286 452 bonds / 9 469 particles = **60.5 at 20 k, past the
search's own condensation threshold of 50**. The 20 k probe (headline
2 421.601, matching the committed record to all digits) decomposes its
best rows, and they tell the same story at structure level: the headline
row is a 102-member web at **mean degree 66.7** (P 196 × C 12.355 =
4.625 + 3.516 + 4.215, tick 19 600), and the peak-complexity row sits at
mean degree 75.3. The trajectory explains the mechanism: a web grows as
a near-clique (at tick 10 100 the headline structure is a *perfect*
56-clique — H = 0.000, `d̄` = 55), and its high-entropy moments are
clique-merge events, when two completed cliques briefly share a sparse
bridge. Even the flagship "non-condensing" discovery earns its headline
from structures past the mark.

The corpus 20 k leaderboard, annotated with world mean degree, now reads
as a condensation ladder:

| pack | headline @20 k | world d̄ @20 k |
|---|---:|---:|
| actual | 3 631.4 | 215.4 |
| bands | 2 711.9 | 418.6 |
| champion g006-i009 | 2 421.6 | 60.5 |
| sandbox | 2 178.5 | 9.9 (but giant = 10 459 members, world-spanning) |
| full-stack | 1 806.3 | 2.2 |
| chains | 1 196.4 | 0.8 |

Nothing that stays sparse scores above ~1 800; everything above 2 400
has crossed the mark; the top two are an order of magnitude past it.

## Finding 3 — the ceiling: no marked-sparse structure can beat `actual`

The bound. Among integer degree distributions on `{1, 2, …}` with mean
`d̄`, entropy is maximized by the (shifted) geometric distribution, so
`H ≤ H_geom(d̄)`, and `H_geom` is increasing in `d̄`. For a structure
respecting the condensation mark (`d̄ ≤ 50`):

    C  =  ln S + H + ln(1 + d̄)  ≤  ln S + 4.902 + 3.932  =  ln S + 8.834

With persistence granted its maximum (200 samples — alive from the first
sample to the last), beating `actual`'s 3 631.36 needs `C ≥ 18.157`,
hence `ln S ≥ 9.323`: **at least 11 192 members in one connected
structure**. No 20 k corpus population reaches that — finals run 9 469
(champion) to 10 250 (full-stack). Per-population ceilings:

| population S (one structure spanning it all) | C ceiling | headline ceiling |
|---:|---:|---:|
| 9 469 (champion final) | 17.99 | 3 598 |
| 10 052 (actual final) | 18.05 | 3 610 |
| 10 250 (full-stack final) | 18.07 | 3 614 |
| 13 866 (sieve at 3 k, largest population ever observed) | 18.37 | 3 674 |

So even granting a hypothetical structure *every* particle in the world,
a mathematically maximal-entropy degree distribution at exactly the
mark, and unbroken persistence, no population a 20 k corpus run has
actually reached clears 3 631. And the max-entropy grant is enormous:
the highest degree entropy ever measured in any probe is **3.56** (the
sandbox giant), not 4.90. At observed entropy the requirement becomes
`ln S ≥ 10.67` — **S ≥ 42 957, four times any population ever
simulated**. Meanwhile `actual`'s and `bands`' committed complexity
peaks (21.12, 22.23) sit *above* the entire marked-sparse ceiling
region: the top of the scalar is reachable only by structures whose
mean degree grows far past the mark, because two of the three terms —
connectivity outright, and entropy through its `H_geom(d̄)` envelope —
are priced in degree growth, and condensation is the only dynamic that
buys unbounded degree.

## Finding 4 — what the scalar leaves open, exactly

The one theoretical non-condensed path to 3 631+: grow the population
past ~11 200 (emit-heavy dynamics can — sieve reached 13 866 at 3 k),
percolate essentially **all** of it into one connected structure while
holding every mean degree at ≈50, keep the degree histogram near the
max-entropy geometric shape, and hold that world-spanning web together
from the first sample to the last. Its ceiling at sieve's population is
3 674 — a 1.2 % margin over `actual`, before any dynamical realism
discount (observed entropy alone erases it). The nearest existing
regime, sandbox's world-spanning giant (10 459 members, C 15.90),
lands at 2 178 — 40 % short. This is a corner of configuration space,
not a research direction; the honest reading is that the raw scalar's
summit belongs to condensers.

## Consequence — the criterion and the search now pull in opposite directions

Fitness v1 was ratified (Q-2026-07-13-C) to *discount* condensation —
saturating terms, deliberately — while the phase exit criterion is
scored on a scalar whose top region this analysis shows is
condensation-priced. The search is therefore structurally forbidden
from chasing the very quantity the exit criterion demands: search-02's
champion beat the whole corpus on fitness and still sits 33 % below
`actual` on the headline, and finding 3 shows no fitness-v1-shaped
(bounded-degree) regime can close that gap at observed populations.
This is the evidence the 2026-07-14 shift log said should accumulate
before an owner-level look; it is now parked as **Q-2026-07-15-A**
(QUESTIONS.md) with options and a recommendation. Until it is answered,
searches remain worth running — fitness itself is a meaningful
objective and searches keep discovering regime classes (search-02
finding 2) — but "beat every shipped pack on the raw headline" should
not be treated as reachable by a condensation-discounting search, and
no further search should be judged failed for not reaching it.

## Reproduce

Commands above; `anatomy.py` in the artifact directory extracts and
decomposes the argmax rows from any timeline dump. Same build ⇒ every
probe reproduces its committed record's headline to all digits.
