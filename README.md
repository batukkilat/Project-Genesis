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
- **Phase 4 — Environment & planet**: adaptive-detail LOD groundwork (quiet chunks tick less, conservation exact, ~10M baseline); generic environment fields on their own coarse grid, gating rules via `env_cond` (configs/env-gradient.ron + packs/bands.ron); field dynamics (diffusion + relaxation, default-off); player action stream — tick-stamped, replay-recorded actions, run headless via `--actions`: environment edits (scripts/terraform-west.ron), asteroid impacts — momentum + energy shock plus a quantity-range particle payload (scripts/bombardment.ron) — tectonic rifts, the same shock off a world-coordinate segment (scripts/rift.ron) — and planet rotation as frame spin: a Coriolis deflection applied as an exact, energy-preserving velocity rotation, set by the `SpinSet` verb (scripts/spin-up.ron, [docs/research/rotation.md](docs/research/rotation.md)). Exit criteria pass ✅; the magnetic-field verb stays parked until radiation exists (QUESTIONS.md); chunk streaming deferred until an out-of-memory target exists.
- **Phase 5 — Observer**: read-only `genesis-observer` crate — bond-graph structures with stable observer ids, metrics v1 (persistence, stability, complexity, information), two confidence-scored hypotheses (*possibly self-maintaining*, *possibly growing*), timeline recording dumped as RON (`--report`/`--timeline`). Observer on/off provably yields identical state hashes. Exit review passed ([docs/research/phase5-exit-review.md](docs/research/phase5-exit-review.md)). ✅
- **Phase 6 — Rendering & player experience** (underway): snapshot mechanism settled (lockstep with a `RenderFrame` extraction seam — [docs/research/render-bootstrap.md](docs/research/render-bootstrap.md)); `genesis-render` extraction core landed (zoom tiers, torus-correct camera-space sprites and bonds — wrapped-copy tiling for wide views, heatmap cell aggregation, swappable RON visual mappings — Bevy-free and headless-tested) plus the headless logic halves of the interactive steps: heatmap raster (palette ramps in [palettes/](palettes/), aggregates → RGBA8 with ordered dithering), warp pacer (whole-tick frame plans, honest starvation flag), field brush (a stroke → seam-wrapped region edits fed through `Simulation::queue_action` — live play and scripts are one representation), inspector (torus picking, structure lookup, circular-mean camera focus), and timeline branching (`genesis branch`: fork a save into an independent branch with a RON ancestry record, never replay identity). **Windowed app shell landed**: `genesis-app` (Bevy, behind the `app` feature) — lockstep frame loop over the owned simulation, T0/T1 sprites + bond lines, T2/T3 pixel heatmaps (1/4-res raster, integer upscale), pause/warp with target-vs-achieved honesty display, and a live field brush emitting replay-recorded actions. Next: observer/inspector panels and save/load/branch UI.

## Workspace

| Crate | Layer |
|---|---|
| `genesis-core` | Primitives: particle id, 2D vector, torus math, deterministic RNG, state hash. Dependency-free. |
| `genesis-sim` | ECS world (Bevy ECS, headless): physics kernel, interaction engine, bond storage, snapshots. |
| `genesis-config` | RON simulation config, rule-pack, and action-script authoring schemas, validated; part of replay identity. |
| `genesis-persist` | Versioned binary save/load with integrity hash (format v15: physics params incl. frame spin, LOD policy, env fields + dynamics, pending actions incl. impacts, rifts, and spin changes, rules, bonds, dynamic population); timeline branch records (RON sidecar ancestry, above the engine). |
| `genesis-observer` | Layer 5 (read-only): bond-graph structures with stable ids, metrics, confidence-scored hypotheses, timeline. Cannot mutate simulation state. |
| `genesis-render` | Phase 6 rendering: read-only extraction core (snapshot → `RenderFrame`: sprites / bonds / cell aggregates by zoom tier), RON visual mappings, heatmap rasterizer (palettes, Bayer dither), warp pacer, field brush, inspector picking/focus — all headless-tested. The windowed `genesis-app` bin (full Bevy) lives behind the `app` feature so workspace tests stay Bevy-free. |
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
cargo run -p genesis-headless --release -- verify --config configs/full-stack.ron --rules packs/bands.ron --actions scripts/full-stack.ron --ticks 3000
cargo run -p genesis-headless --release -- verify --actions scripts/spin-up.ron --ticks 4000
cargo run -p genesis-headless --release -- score --rules packs/chains.ron --ticks 20000 --every 100 --out chains.score.ron
cargo run -p genesis-render --release --features app --bin genesis-app -- --config configs/env-gradient.ron
cargo run -p genesis-render --release --features app --bin genesis-app -- --smoke 120   # window smoke test
```

Under WSLg the only adapter is llvmpipe (software Vulkan) — it works but renders
on the CPU. The app caps itself at 30 fps (`--fps` to change; the simulation
rate is cap-independent, the pacer just runs more ticks per frame) and drops to
4 fps when the window is unfocused; `LP_NUM_THREADS=4` bounds llvmpipe's worker
threads. For real GPU rendering run natively on Windows:
[tools/run-app.ps1](tools/run-app.ps1) builds and runs the same checkout with
DX12 (one-time setup commands in the script header).

Determinism contract: same build + same platform + same seed/config/rules/actions ⇒ identical state hashes — regardless of thread count. Verified by `genesis verify` (two fresh runs + save/resume + single-thread, all compared) and the test suite. Current numbers: [BASELINES.md](BASELINES.md).

## System requirements

Grounded in the measured baselines ([BASELINES.md](BASELINES.md)); the
simulation is CPU-bound and scales with cores (4.7× on 12 threads at 1M
particles), the renderer is a light GPU consumer.

|  | Minimum | Recommended |
|---|---|---|
| **Use** | Watch/play default worlds (10k–100k particles, real-time) | Research scale (1M–10M particles, warp) |
| **OS** | Windows 10/11 (native or WSL2) or Linux x86_64 | Windows 11 / Linux; macOS should work (wgpu Metal) but is untested |
| **CPU** | 4 cores | 12+ threads — 1M particles runs 12.4 ticks/s on 12 threads; more cores = more warp |
| **RAM** | 8 GB | 16–32 GB — a 10M-particle world plus snapshot/sort scratch wants several GB to itself |
| **GPU** | Any Vulkan/DX12/Metal device; falls back to software rendering (llvmpipe — works, eats CPU, app self-caps at 30 fps) | Dedicated GPU for the windowed app; headless needs none at all |
| **Disk** | ~20 GB free for toolchain + build (release keeps debug symbols; the target dir grows large) | SSD — the drvfs `/mnt/c` path is slow on WSL, keep `CARGO_TARGET_DIR` on the Linux filesystem |
| **Toolchain** | Rust 1.96.1 (pinned by rust-toolchain.toml; rustup installs it automatically) | same |

Determinism is guaranteed per build + platform, so any machine reproduces its
own runs exactly; matching hashes *across* machines is a non-goal (decisions
log). Headless (`genesis-headless`) has zero GPU/display requirements — it runs
in bare containers, which is how the 10M baselines were produced.
