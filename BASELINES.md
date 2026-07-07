# Performance baselines

Tracked results of `genesis bench` per phase. Update when the engine or the
reference machine changes; regressions against the last entry need a reason.

Reference machine: WSL2, 12 threads (`rtk`/dev box), release build, engine v0.2.0.

## Phase 4 (2026-07-07) — adaptive detail (LOD), ~10M baseline

Machine: cloud container, 4 threads, release, engine v0.3.0. The gate before
any environment feature (Q-2026-07-06-A, item 3): LOD-on vs LOD-off throughput
at scale on a representative mostly-quiet world.

Config: `configs/bench-quiet.ron` — dense (4096², r=8 → ~38 particles/cell at
10M, an expensive force pass) but gentle (repulsion 0.5, attraction 0.1, speed
≤ 0.1), so most chunks start below the cold-rung threshold. Ladder
`[(0,8),(0.25,4),(1,1)]`, `chunk_cells=8`. Physics only (no rule pack).

Command: `genesis bench --config configs/bench-quiet.ron --particles N --ticks T [--no-lod]`

| N | LOD | Particle-ticks/s | Active frac | Speedup | State hash |
|---|---|---|---|---|---|
| 10M | off | 1.42e6 | — | 1x | `0x4fdf0260daa242ea` |
| 10M | on | 1.82e6 | 0.730 | 1.28x | `0xaf325a58a255ffd4` |
| 3M | off | 3.49e6 | — | 1x | `0x9a492d417111f795` |
| 3M | on | 6.16e6 | 0.365 | 1.77x | `0x8555f741a4f22623` |

Notes:

- **LOD works and is exactly conservative.** Determinism (LOD-on self-identical
  across thread counts and save/resume) and matter/energy conservation are
  proven by tests and `genesis verify --config configs/lod.ron` (not by these
  throughput rows). LOD-on and LOD-off hash differently *by design* — a
  different policy is a different universe; the speedup is the only thing these
  rows measure.
- **Speedup tracks the active fraction, which rises with density.** At 3M
  (~11/cell) the world stays quieter (0.365 active → 1.77x); at 10M (~38/cell)
  stronger cumulative forces heat more chunks over the run (0.730 active →
  1.28x). LOD pays off most in the dense-*and*-quiet regime — settled emergent
  structures — and least where the world is either dilute (force pass already
  cheap; measured: no speedup at 1M in a sparse world) or hot (few cold chunks).
- **`canonicalize` is an unskipped fixed cost that bounds the speedup.** The
  per-tick (cell, id) sort runs over every particle regardless of LOD, so LOD
  only ever removes the force/interaction/integrate work of frozen particles,
  not the sort. Frozen particles do not move, so their cell is unchanged — an
  incremental/bucketed re-sort that skips stable regions (already flagged in the
  Phase 2 notes below) is the natural next optimization to unlock more of LOD's
  potential. Recorded as a known limitation, not a regression.
- Active fraction is the mean over sampled ticks; with an all-rate-8 cold set it
  cycles (all-active every 8th tick, near-zero between), so the mean understates
  the off-stride savings and overstates the on-stride cost.

## Phase 3 (2026-07-06) — interaction overhead, secondary machine

Machine: cloud container, 4 threads, release, engine v0.3.0. **Not
comparable to the WSL reference rows** — recorded as the first
interaction-engine datapoints; the *ratios* are the transferable part.

Command: `genesis bench --particles 1000000 --ticks 120 [--threads 1] [--rules packs/X.ron]`

| Run | Threads | Particle-ticks/s | vs bare physics | State hash |
|---|---|---|---|---|
| physics only | 4 | 5.53e6 | 1x | `0xab3fdfe92d0385b6` |
| physics only | 1 | 1.96e6 | — | `0xab3fdfe92d0385b6` |
| packs/actual.ron | 4 | 2.97e4 | ~186x slower | `0x82c8703f73d4a98c` |
| packs/chains.ron | 4 | 7.43e3 | ~744x slower | `0x0df814a2768bfa29` |

Notes:

- The default bench world (4096², r=8) at 1M particles is the dense
  regime: ~3.8 particles/cell, ~34 candidate neighbors each. Packs pay per
  candidate pair per rule (condition checks + RNG stream derivation), and
  both packs mass-create bonds at this density (actual.ron runs away into
  a gel — docs/research/phase3-exit-review.md), so these are worst-case
  numbers, not typical-content numbers. Wall clock for the two pack rows:
  67 min and 4.5 h per 120 ticks.
- The point of the ratios: un-indexed rule matching at high density costs
  2–3 orders of magnitude over bare physics. Performance.md's "rule
  matching scales" risk is now measured, and it is hard justification for
  the settled Phase 4 ordering (adaptive detail first) plus the rule-index
  / half-neighborhood candidates from the Phase 2 notes.
- Physics thread-count invariance re-confirmed on this box at 1M (4-thread
  and 1-thread hashes identical). Pack rows are single runs; their
  determinism is covered per-pack by `genesis verify` in the exit review.
- Hashes above predate the `information_max` cap (save format v7): pack
  rows re-run on a current build will hash differently (still
  deterministically). Throughput ratios are unaffected.

## Phase 2 (2026-07-05)

Command: `genesis bench --particles 1000000 --ticks 120 [--threads N]`

| Threads | Spawn | Throughput | Particle-ticks/s | State hash |
|---|---|---|---|---|
| 12 | 35 ms | 12.4 ticks/s | 1.24e7 | `0x92be7fb5c83b10fb` |
| 1 | 32 ms | 2.6 ticks/s | 2.64e6 | `0x92be7fb5c83b10fb` |

Notes:

- Hash identical across thread counts — determinism under parallelism holds at 1M scale.
- Default config: 4096x4096 world, r=8 kernel → ~3.8 particles/cell, ~34 neighbors each.
- Scaling 4.7x on 12 threads; the canonical (cell,id) sort and memory bandwidth bound the rest. Candidates when Phase 8-style optimization matters: incremental/bucketed re-sort, half-neighborhood force evaluation (Newton's third law), SIMD kernel.

## Phase 1 (2026-07-05, engine v0.1.0)

Inert particles (no physics): loop overhead only, 330k ticks/s at 1M particles.
