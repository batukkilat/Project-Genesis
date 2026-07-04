# Performance baselines

Tracked results of `genesis bench` per phase. Update when the engine or the
reference machine changes; regressions against the last entry need a reason.

Reference machine: WSL2, 12 threads (`rtk`/dev box), release build, engine v0.2.0.

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
