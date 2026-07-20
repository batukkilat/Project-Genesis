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

Machine: 4-core cloud box, release build at `e271bf3` (rustc 1.96.1);
the 20 k half ran 2026-07-20 at `842da05` (code-identical — the
intervening commits are docs and a sweep spec) after the 2026-07-17
container cut ended the session mid-measurement. Every re-scored run
reproduced its committed state hash bit-for-bit (12/12 at the 3 k
gate, 6/6 at 20 k), which is the on-line proof that the field cannot
touch a simulated bit: the observer reads snapshots, and the universe
it read is byte-identical to the one scored before the field existed.

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

The affordable subset (wall times measured this run, sequential on the
4-core box; every hash = the committed record):

| run | raw headline | bounded | gap | bonds (final) | wall |
|---|---:|---:|---:|---:|---:|
| actual | 3631.36 | **473.99** | −87.0 % | 1 082 617 | 1221 s |
| bands | 2711.93 | *unmeasured* | — | 2 092 763 | (~107 min, skipped) |
| g006-i009 (search-02) | 2421.60 | 2106.60 | −13.0 % | 286 452 | 657 s |
| g008-i003 (search-04) | 2192.42 | 1894.20 | −13.6 % | 58 297 | 262 s |
| sandbox | 2178.48 | 2178.48 | — | 50 038 | 590 s |
| full-stack | 1806.26 | 1806.26 | — | 11 151 | 145 s |
| chains | 1196.38 | 1196.38 | — | 4 239 | 62 s |
| sieve | — | *unmeasured* | — | — | (>3.5 h, skipped) |

Sorted by the raw scalar. Re-sorted by the **bounded** scalar the
measured leaderboard reads: sandbox 2178.5 > g006-i009 2106.6 >
g008-i003 1894.2 > full-stack 1806.3 > chains 1196.4 > actual 474.0.

## Findings

1. **The anatomy doc's claim, now measured.** `actual` — the raw
   scalar's leader and the bar the exit criterion sets — keeps only
   13 % of its headline when condensed rows are excluded: 3631 → 474.
   Its top (structure, sample) rows are welded blobs (1.08 M bonds on
   ~10 k particles), exactly what the 2026-07-15 analytic ceilings
   predicted. The raw exit bar is priced almost entirely in the regime
   class the phase's own fitness rejects.

2. **The bound is inert for honest regimes at every measured horizon.**
   sandbox, full-stack, and chains keep their raw values to the last
   digit at 20 k, as 11 of 12 packs did at 3 k: the mark only ever
   subtracts from regimes that actually condense. Adopting option 2
   would not reshuffle honest packs — it removes exactly one thing,
   the condensation premium.

3. **Under the bounded scalar the criterion becomes contestable by the
   discovered regimes.** The measured bounded bar is sandbox's 2178.5.
   The search-02 champion sits 3.3 % below it (2106.6); the search-04
   champion 13 % below (1894.2). Against the raw bar (actual's 3631)
   the best discovered regime is 33 % short with an analytic proof it
   cannot close the gap un-condensed; against the bounded bar the gap
   is 3.3 % with no structural obstacle — the "corner of configuration
   space" the anatomy doc pointed at, now with a measured distance.

4. **The champions themselves pay the bound.** Both lose ~13 %: their
   peak rows partially condense between the 3 k screen (where
   g006-i009's gap was zero) and 20 k — consistent with the known
   3 k → 20 k crossing of the search-02 champion. The bounded scalar
   is not a flag handed to the search lineages; it prices condensation
   out of *every* regime, discovered or shipped.

5. **Caveat: the two unmeasured rows.** bands (raw 2711.93, 2.09 M
   bonds) and sieve have no 20 k bounded value — 107 min and >3.5 h
   respectively at the recorded baselines. bands' bond mass makes a
   high bounded value unlikely but that is inference, not measurement.
   If the owner adopts option 2, bands' bounded row is the one number
   worth buying before re-baselining the criterion bar.

## Reproduce

```sh
genesis sweep --spec sweeps/shipped-packs-3k.ron --out <dir>
genesis score --rules packs/actual.ron --ticks 20000 --every 100 --out actual.20k.score.ron
# …and likewise per 20 k row, with the config/rules/actions stamped in
# each committed record (champions: the g006-i009 / g008-i003 files in
# their search directories).
```
