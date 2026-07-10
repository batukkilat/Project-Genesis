# Project Genesis

An Artificial Life Research Sandbox: the emergence of complexity from first principles. Particles and interactions only — everything else must emerge.

> Don't simulate Earth.
> Simulate possibility.

- **[Prompts/MASTER_PROMPT.md](Prompts/MASTER_PROMPT.md)** — the constitution: vision, immutable rules, architecture.
- **[ROADMAP.md](ROADMAP.md)** — the phased plan (canonical). Phases 1–3 complete; Phase 4 (environment) and Phase 5 (Observer) exit criteria pass; Phase 6 (rendering) underway — extraction core landed, windowed app next.
- **[Prompts/spec/](Prompts/spec)** — per-system specifications.
- **[packs/](packs)** — authored interaction rule packs (RON); a pack is content, not code.

## Status

- **Phase 1 — Foundation**: workspace, deterministic RNG (SplitMix64 + order-free derived streams), state hashing, fixed-timestep ECS loop, versioned saves. ✅
- **Phase 2 — Physics & space**: torus world, canonical (cell, id) SoA layout, generic short-range kernel, chunk-parallel forces with thread-count-invariant hashes (proven at 1M particles — [BASELINES.md](BASELINES.md)). ✅
- **Phase 3 — Interactions & chemistry**: data-driven rule engine (condition → probability → action), quantity transfers, RON rule-pack authoring, bonds (canonical edge list + per-tick CSR mirror, harmonic spring forces, rule-driven create/break), lossy information copy + decay (information deliberately non-conserved), particle emit/absorb (split/merge, conserved per event, ids never reused). Two-regime demo (packs/actual.ron vs packs/sandbox.ron — same engine, opposite economies). Exit review passed ([docs/research/phase3-exit-review.md](docs/research/phase3-exit-review.md)): persistent uncoded structures over 20k ticks, deterministic and conserved throughout. ✅
- **Phase 4 — Environment & planet**: adaptive-detail LOD groundwork (quiet chunks tick less, conservation exact, ~10M baseline); generic environment fields on their own coarse grid, gating rules via `env_cond` (configs/env-gradient.ron + packs/bands.ron); field dynamics (diffusion + relaxation, default-off); player action stream — tick-stamped, replay-recorded actions, run headless via `--actions`: environment edits (scripts/terraform-west.ron), asteroid impacts — momentum + energy shock plus a quantity-range particle payload (scripts/bombardment.ron) — and tectonic rifts, the same shock off a world-coordinate segment (scripts/rift.ron). Exit criteria pass ✅; remaining player verbs (rotation, magnetic field) are parked design forks (QUESTIONS.md); chunk streaming deferred until an out-of-memory target exists.
- **Phase 5 — Observer**: read-only `genesis-observer` crate — bond-graph structures with stable observer ids, metrics v1 (persistence, stability, complexity, information), two confidence-scored hypotheses (*possibly self-maintaining*, *possibly growing*), timeline recording dumped as RON (`--report`/`--timeline`). Observer on/off provably yields identical state hashes. Exit review passed ([docs/research/phase5-exit-review.md](docs/research/phase5-exit-review.md)). ✅
- **Phase 6 — Rendering & player experience** (underway): snapshot mechanism settled (lockstep with a `RenderFrame` extraction seam — [docs/research/render-bootstrap.md](docs/research/render-bootstrap.md)); `genesis-render` extraction core landed (zoom tiers, torus-correct camera-space sprites and bonds — wrapped-copy tiling for wide views, heatmap cell aggregation, swappable RON visual mappings — Bevy-free and headless-tested) plus the headless logic halves of the interactive steps: heatmap raster (palette ramps in [palettes/](palettes/), aggregates → RGBA8 with ordered dithering), warp pacer (whole-tick frame plans, honest starvation flag), field brush (a stroke → seam-wrapped region edits fed through `Simulation::queue_action` — live play and scripts are one representation), inspector (torus picking, structure lookup, circular-mean camera focus), and timeline branching (`genesis branch`: fork a save into an independent branch with a RON ancestry record, never replay identity). Next: the windowed Bevy app shell (needs a display).

## Workspace

| Crate | Layer |
|---|---|
| `genesis-core` | Primitives: particle id, 2D vector, torus math, deterministic RNG, state hash. Dependency-free. |
| `genesis-sim` | ECS world (Bevy ECS, headless): physics kernel, interaction engine, bond storage, snapshots. |
| `genesis-config` | RON simulation config, rule-pack, and action-script authoring schemas, validated; part of replay identity. |
| `genesis-persist` | Versioned binary save/load with integrity hash (format v14: physics params, LOD policy, env fields + dynamics, pending actions incl. impacts and rifts, rules, bonds, dynamic population); timeline branch records (RON sidecar ancestry, above the engine). |
| `genesis-observer` | Layer 5 (read-only): bond-graph structures with stable ids, metrics, confidence-scored hypotheses, timeline. Cannot mutate simulation state. |
| `genesis-render` | Phase 6 extraction core (read-only): snapshot → `RenderFrame` (sprites / bonds / cell aggregates by zoom tier), RON visual mappings, heatmap rasterizer (palettes, Bayer dither), warp pacer, field brush, inspector picking/focus. Bevy arrives with the app shell. |
| `genesis-headless` | CLI: run (with `--report` diagnostics, `--observer` config, `--timeline` RON dump), verify determinism, bench, branch (fork a save + ancestry record), init-config, init-rules. |

## Quick start

```sh
cargo test                                  # all unit + integration tests
cargo run -p genesis-headless --release -- verify --ticks 1000
cargo run -p genesis-headless --release -- verify --rules packs/chains.ron --ticks 1000
cargo run -p genesis-headless --release -- bench --particles 1000000 --ticks 120
cargo run -p genesis-headless --release -- bench --particles 1000000 --ticks 120 --threads 1
cargo run -p genesis-headless --release -- init-config genesis.ron
cargo run -p genesis-headless --release -- init-rules my-pack.ron
cargo run -p genesis-headless --release -- run --config genesis.ron --rules packs/diffusion.ron --ticks 5000 --save world.gens
cargo run -p genesis-headless --release -- run --load world.gens --ticks 5000
cargo run -p genesis-headless --release -- branch --from world.gens --to fork.gens
cargo run -p genesis-headless --release -- verify --config configs/env-gradient.ron --rules packs/bands.ron --actions scripts/terraform-west.ron --ticks 3000
```

Determinism contract: same build + same platform + same seed/config/rules/actions ⇒ identical state hashes — regardless of thread count. Verified by `genesis verify` (two fresh runs + save/resume + single-thread, all compared) and the test suite. Current numbers: [BASELINES.md](BASELINES.md).
