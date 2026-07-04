# Project Genesis

An Artificial Life Research Sandbox: the emergence of complexity from first principles. Particles and interactions only — everything else must emerge.

> Don't simulate Earth.
> Simulate possibility.

- **[Prompts/MASTER_PROMPT.md](Prompts/MASTER_PROMPT.md)** — the constitution: vision, immutable rules, architecture.
- **[ROADMAP.md](ROADMAP.md)** — the phased plan (canonical). Currently: Phase 1 (foundation).
- **[Prompts/spec/](Prompts/spec)** — per-system specifications.

## Workspace

| Crate | Layer |
|---|---|
| `genesis-core` | Primitives: particle id, 2D vector, deterministic RNG, state hash. Dependency-free. |
| `genesis-sim` | ECS world (Bevy ECS, headless), fixed-timestep loop, deterministic spawn, snapshots. |
| `genesis-config` | RON configuration, validated; part of replay identity. |
| `genesis-persist` | Versioned binary save/load with integrity hash. |
| `genesis-headless` | CLI: run, verify determinism, bench, init-config. |

## Quick start

```sh
cargo test                                  # all unit + integration tests
cargo run -p genesis-headless --release -- verify --ticks 1000
cargo run -p genesis-headless --release -- bench --particles 1000000 --ticks 120
cargo run -p genesis-headless --release -- bench --particles 1000000 --ticks 120 --threads 1
cargo run -p genesis-headless --release -- init-config genesis.ron
cargo run -p genesis-headless --release -- run --config genesis.ron --ticks 5000 --save world.gens
cargo run -p genesis-headless --release -- run --load world.gens --ticks 5000
```

Determinism contract: same build + same platform + same seed/config/actions ⇒ identical state hashes — regardless of thread count. Verified by `genesis verify` (two fresh runs + save/resume + single-thread, all compared) and the test suite. Current numbers: [BASELINES.md](BASELINES.md).
