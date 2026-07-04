# PROJECT GENESIS — ROADMAP

Companion to Prompts/MASTER_PROMPT.md (the constitution). This file is the plan and is expected to change; the constitution is not. Phases are sequential but later phases may begin once the previous phase's exit criteria pass.

## Decisions log

Settled — do not re-litigate without cause:

- **Determinism scope:** same platform + same build. Cross-platform bit-exactness is a non-goal.
- **Space:** 2D torus — continuous f32 positions, both axes wrap, no edges.
- **Physics vs interactions (hybrid):** Physics = hardcoded generic continuous operators (motion, forces, diffusion, heat), config-parameterized. Interaction System = discrete data-driven events. Both deterministic, neither Earth-specific.
- **Create/destroy:** interactions may create and destroy particles (split, merge, emit, absorb); matter+energy conserved per event; ids never reused.
- **Time:** fixed dt forever; time warp = more ticks per wall second, never a bigger dt.
- **Save format:** hand-rolled versioned binary (`GENS`, format v1) — no serialization dependency in the format.

## Phase 1 — Foundation (current)

Defined in MASTER_PROMPT.md. Workspace, headless simulation, particle type, deterministic RNG, fixed-timestep loop, ECS, config, logging, save/load framework, tests.

Proposed workspace layout (one crate per layer, dependencies flow downward only):

```
genesis/
  crates/
    genesis-core/      # particle type, fundamental quantities, deterministic RNG, state hash
    genesis-sim/       # simulation loop, scheduling, chunk framework, ECS integration
    genesis-config/    # simulation + rule configuration, validation
    genesis-persist/   # versioned binary save/load, replay recording
    genesis-headless/  # CLI binary: run, benchmark, replay-verify
  Cargo.toml           # workspace root
  rust-toolchain.toml
```

Renderer, observer, and AI crates are added in their own phases — not scaffolded empty now.

## Phase 2 — Physics and Space

Goal: particles move, collide, and exchange matter/energy under conservation laws, in parallel, deterministically.

Deliverables:

- Spatial partitioning: chunk grid, neighbor queries, particle migration between chunks. Torus wrapping in all position math.
- Motion integration (fixed timestep), collision/force resolution.
- Conservation accounting: total matter and energy tracked per tick; leaks fail tests.
- Multithreaded chunk simulation using the two-phase collect-then-commit rule from the constitution.
- Profiling harness and benchmark suite (criterion or similar) with tracked baselines.
- Adaptive simulation detail groundwork: chunks can run at reduced rates without breaking determinism.

Exit criteria: millions of interacting particles; state hash identical between single-threaded and multithreaded runs of the same seed; conservation tests pass over long runs.

Non-goals: chemistry, bonds, any interaction beyond physical forces.

## Phase 3 — Interaction System and Chemistry

Goal: the data-driven interaction engine — the heart of emergence.

Deliverables:

- Interaction rule format (condition → action → probability → costs → transfers) as data files, validated at load, hashed into the replay identity (a rule change is a version change).
- Bonds between particles; compound structures as pure consequences of bonds, never as objects the engine names.
- Matter/energy/information transfers through interactions.
- Particle creation and destruction through interactions (split, merge, emit, absorb), conserved per event; ids never reused.
- First concrete semantics for the information quantity (open design question — see below).
- Actual Physics and Sandbox Physics as two rule packs on the same engine, not two code paths.

Exit criteria: a nontrivial rule pack produces persistent multi-particle structures nobody explicitly coded; determinism and conservation still hold.

Non-goals: anything that names biology; environment simulation.

## Phase 4 — Environment and Planet

Goal: the world the player actually manipulates.

Deliverables:

- Planet-scale environment fields: temperature, pressure, gravity, radiation, atmosphere composition — sampled by chunks, influencing interaction conditions.
- Player environment tools (the only player verbs): adjust fields, rotation, magnetic field, tectonic events, asteroid impacts, time scale.
- Player actions recorded in the replay stream (constitution rule 6 covers them).
- Chunk streaming / persistence so planets exceed memory.

Exit criteria: a player script (headless) can shape an environment and replay it identically; environment gradients visibly shape where structures from Phase 3 emerge.

Non-goals: rendering, UI.

## Phase 5 — Emergent Structures and Observer

Goal: detect what emerged, without ever influencing it.

Deliverables:

- Observer as a separate crate that consumes read-only snapshots; physically cannot mutate simulation state.
- Structure detection and tracking over time (clusters, cycles, self-maintaining patterns).
- Metrics: complexity, entropy, information density, persistence.
- Hypothesis system: "possibly self-replicating", "possibly homeostatic" — confidence-scored, never absolute.
- History/timeline recording for later narration.

Exit criteria: Observer flags structures in Phase 3/4 outputs that match what developers see by eye; simulation state hash provably unaffected by Observer presence.

Non-goals: natural-language output; rendering.

## Phase 6 — Rendering and Player Experience

Goal: make it watchable and playable.

Deliverables:

- Bevy renderer as a pure consumer of simulation snapshots (renderer never owns simulation logic).
- Retro pixel art, top-down camera, continuous zoom planet → particle with rendering LOD independent of simulation LOD.
- Environment-tool UI, time controls, Observer overlay (hypotheses, metrics, timelines).
- Save/load/replay UI.
- Timeline branching: fork a run into an independent save + player-action log with shared ancestry metadata.

Exit criteria: full loop — empty planet, shape environment, run, observe, experiment — with no objectives and no loading screens in the common path.

Non-goals: AI narration.

## Phase 7 — AI Narration and Hardening

Goal: the storyteller layer, plus release-quality robustness.

Deliverables:

- AI narration consuming Observer output only: summaries, reports, Q&A about a run's history. Engine remains fully functional with AI disabled.
- Shareable replays (seed + config + rule-pack hash + player actions).
- Save-format migration between engine versions.
- Performance hardening against Phase 2 baselines; long-run soak tests.

Exit criteria: a complete run can be narrated as a history; disabling AI changes nothing but the narration.

## Open design questions

Decide when the phase that needs them starts, not before:

1. **Information semantics** (Phase 3): what does a particle's information quantity actually do — copyable at a cost? decays? gates interactions? This is the single most emergence-critical decision in the project.
2. **Float policy** (Phase 2): same-machine determinism allows plain f32/f64, but parallel reductions still need ordering discipline; decide summation strategy when conservation accounting is built.
3. **Rule format** (Phase 3): custom binary/RON schema vs. embedded scripting for conditions. Bias toward data, not scripts — scripts make replay identity and sandboxing harder.
4. **Snapshot mechanism** (Phase 5/6): how Observer and renderer read state without stalling the simulation (double-buffering vs. copy-on-write).
5. **Bond storage** (Phase 3): particle graphs are cache-hostile; decide layout (adjacency in components vs. separate bond store) when bonds land.
