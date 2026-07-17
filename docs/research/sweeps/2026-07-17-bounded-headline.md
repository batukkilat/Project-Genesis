# 2026-07-17 — the bounded-degree headline, measured

**Question.** Q-2026-07-15-A option 2 proposes scoring the Phase 6.5
exit criterion on a *bounded-degree headline*: the same
max persistence × complexity, taken only over (structure, sample) rows
whose mean bond degree respects the condensation mark (≤ 50). The
headline-anatomy analysis (2026-07-15) argued from analytic ceilings
that the raw scalar is a condensation ladder; this session shipped the
field itself (`persistence_complexity_bounded`, decisions log
Q-2026-07-17-B) and re-scored what the budget allowed, so the parked
owner decision can be made against measured numbers instead of bounds.

Machine: 4-core cloud box, release build at `e271bf3` (rustc 1.96.1).
Every re-scored run reproduced its committed state hash bit-for-bit
(12/12 at the 3 k gate, 6/6 at 20 k), which is the on-line proof that
the field cannot touch a simulated bit: the observer reads snapshots,
and the universe it read is byte-identical to the one scored before
the field existed.

## Method

`persistence_complexity_bounded` is computed in the same sample loop
as the raw scalar; a row enters the bounded maximum iff its
per-structure `mean_degree ≤ CONDENSED_MEAN_DEGREE` (= 50, the one
constant now shared with the search's leaderboard flag). Legacy
records load as `None` ("not measured"), never as a fake zero.
Re-scoring is `genesis score` / `genesis sweep` with the committed
stamps' exact parameters (defaults observer, cadence 100).

Budget note (PLAYBOOK §5): `bands` at 20 k costs ~107 min and `sieve`
at 20 k never completed in 3 h 35 m (2026-07-13 baseline, finding 6) —
both skipped; their bounded values at 20 k stay unmeasured. The six
runs below cost ~46 min total by the recorded wall times.

## The 3 k screen horizon: the mark binds almost nowhere

Full table: the 3 k gate re-run (all hashes = committed
2026-07-14/-15 records; zero-score packs omitted).

| run | raw headline | bounded | gap |
|---|---:|---:|---:|
| sandbox | 369.81 | 369.81 | — |
| sieve | 318.55 | 318.55 | — |
| gradient-sieve | 294.50 | 289.51 | −1.7 % |
| bands | 291.47 | 291.47 | — |
| full-stack | 244.86 | 244.86 | — |
| actual | 208.01 | 208.01 | — |
| chains | 105.74 | 105.74 | — |

At the screen horizon the bounded headline *is* the raw headline for
every pack but one: nothing has welded yet (gradient-sieve's top row
is carried by a marginally-over-mark structure, a −1.7 % gap). This is
the anatomy doc's timeline claim shown directly in data: condensation
is a 3 k → 20 k phenomenon, which is also why the search screen keeps
failing to see it coming (Q-2026-07-17-A).

## The 20 k corpus horizon

<!-- 20K-TABLE -->

## Findings

<!-- FINDINGS -->

## Reproduce

```sh
genesis sweep --spec sweeps/shipped-packs-3k.ron --out <dir>
genesis score --rules packs/actual.ron --ticks 20000 --every 100 --out actual.20k.score.ron
# …and likewise per 20 k row, with the config/rules/actions stamped in
# each committed record (champions: the g006-i009 / g008-i003 files in
# their search directories).
```
