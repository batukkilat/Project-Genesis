# Folder Structure
Actual layout (Cargo workspace):

crates/genesis-core/      # L0 primitives: particle id, vec2, RNG, state hash. Dependency-free.
crates/genesis-sim/       # ECS world, fixed-timestep loop, snapshots (+ physics, interactions as phases land)
crates/genesis-config/    # RON config, validated; part of replay identity
crates/genesis-persist/   # versioned binary save/load
crates/genesis-headless/  # CLI: run, verify, bench, init-config
Prompts/                  # constitution (MASTER_PROMPT.md), specs, build prompts
ROADMAP.md                # canonical phased plan (repo root)

Added by later phases:
crates/genesis-observer/  # Phase 5
crates/genesis-render/    # Phase 6 (Bevy)
crates/genesis-narrate/   # Phase 7 (AI)

Tests live inside each crate (unit + integration).
