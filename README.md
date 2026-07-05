# Project Genesis

An Artificial Life Research Sandbox: the emergence of complexity from first principles. Particles and interactions only — everything else must emerge.

> Don't simulate Earth.
> Simulate possibility.

- **[Prompts/MASTER_PROMPT.md](Prompts/MASTER_PROMPT.md)** — the constitution: vision, immutable rules, architecture.
- **[ROADMAP.md](ROADMAP.md)** — the phased plan (canonical). Phases 1–2 complete; currently: Phase 3 (interactions & chemistry).
- **[Prompts/spec/](Prompts/spec)** — per-system specifications.
- **[packs/](packs)** — authored interaction rule packs (RON); a pack is content, not code.

## Status

- **Phase 1 — Foundation**: workspace, deterministic RNG (SplitMix64 + order-free derived streams), state hashing, fixed-timestep ECS loop, versioned saves. ✅
- **Phase 2 — Physics & space**: torus world, canonical (cell, id) SoA layout, generic short-range kernel, chunk-parallel forces with thread-count-invariant hashes (proven at 1M particles — [BASELINES.md](BASELINES.md)). ✅
- **Phase 3 — Interactions & chemistry** (in progress): data-driven rule engine (condition → probability → action), quantity transfers, RON rule-pack authoring, bonds (canonical edge list + per-tick CSR mirror, harmonic spring forces, rule-driven create/break), lossy information copy + decay (information deliberately non-conserved), particle emit/absorb (split/merge, conserved per event, ids never reused). Remaining: the two-packs-one-engine demo, then exit review.

## Workspace

| Crate | Layer |
|---|---|
| `genesis-core` | Primitives: particle id, 2D vector, torus math, deterministic RNG, state hash. Dependency-free. |
| `genesis-sim` | ECS world (Bevy ECS, headless): physics kernel, interaction engine, bond storage, snapshots. |
| `genesis-config` | RON simulation config + rule-pack authoring schema, validated; part of replay identity. |
| `genesis-persist` | Versioned binary save/load with integrity hash (format v4: physics params, rules, bonds). |
| `genesis-headless` | CLI: run, verify determinism, bench, init-config, init-rules. |

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
```

Determinism contract: same build + same platform + same seed/config/rules/actions ⇒ identical state hashes — regardless of thread count. Verified by `genesis verify` (two fresh runs + save/resume + single-thread, all compared) and the test suite. Current numbers: [BASELINES.md](BASELINES.md).
