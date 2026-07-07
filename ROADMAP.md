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
- **Information semantics (2026-07-05):** condition-gate + lossy copy. Rule conditions can read it; interactions can copy it between particles at an energy cost with configurable noise; not conserved (creatable by paying energy, lost by decay). Self-replication becomes possible, never built-in.
- **Rule authoring (2026-07-05):** RON data schema — declarative conditions and a fixed action vocabulary, compiled and validated at load, whole pack hashed into replay identity. No scripting in the hot loop.
- **Bond storage (2026-07-05):** SoA edge list keyed by stable ids (a < b, canonically sorted) as save/hash truth + per-tick CSR adjacency mirror for iteration + lookup-only id→index hash map. See docs/research/bond-storage.md.
- **Asteroids / external material (2026-07-06):** an asteroid impact is a replay-recorded event delivering momentum + energy shock + a particle payload. The payload's "material" is specified as quantity ranges (matter/energy/information distributions) — a region of quantity space, never a named substance. Named materials ("minerals") stay banned below the Observer layer; UI/Observer may label payload profiles. **Parked, decide at Phase 4 design review:** a generic `species` axis on particles (per-pair-species kernel table, Particle-Life-style) — powerful but touches physics, rules, saves, and hashing everywhere; must be its own deliberate amendment, never smuggled in via asteroids.
- **Phase 4 ordering (2026-07-06):** adaptive-detail groundwork (chunks ticking at reduced rates, deferred from Phase 2) is the FIRST Phase 4 work item, validated with a ~10M-particle baseline before environment features land. Planet-level *zoom* is a Phase 6 rendering concern and gets cheaper zoomed out (LOD aggregates); the Phase 4 perf cliff is population scale, not the camera.
- **Adaptive detail exactness (2026-07-06, answers Q-2026-07-06-A: Option A):** approximate LOD with the detail policy part of replay identity. Implementation plan drafted in docs/research/adaptive-detail.md (freeze-based both-active gating, per-tick stateless classification, staged landing order; forks marked there are ratified into this log on landing). Chunks are classified by a state-keyed activity metric; cold chunks run every k-th tick. The policy (metric, thresholds, rate ladder) is configuration, hashed into replay identity like physics params — same seed + config + policy = bit-identical replay; a different policy is a different universe. Hard invariants: (1) the policy is configuration, never derived from machine load, thread count, or wall clock; (2) matter/energy conservation holds exactly at every rate, including across cross-rate chunk boundaries; (3) `genesis verify` grows an LOD mode proving LOD-on runs self-identical across thread counts and save/resume, and the ~10M baseline lands in BASELINES.md before any environment feature starts.
- **Adaptive-detail landing forks (2026-07-07, ratifies the forks in docs/research/adaptive-detail.md; Q-2026-07-07-A):** the groundwork landed in the staged order from that doc. Three forks it left to the implementer are now settled:
  - **Classification: per-tick, stateless (no hysteresis).** The activity mask is a pure function of `(state, policy, tick)` — a per-chunk `max` of speed² picks a tick stride from the ladder, and a particle is active iff its chunk's stride divides the tick. No LOD state is saved, so save/resume is bit-identical for free and thread count cannot change a bit. Possible rate *thrash* at a threshold boundary is a performance-only concern; hysteresis (a saved previous-rate array) is a later refinement if profiling demands it.
  - **Chunk geometry: `chunk_cells × chunk_cells` blocks of grid cells, partial edge chunks allowed.** Chunks are pure `GridGeom` geometry (`chunk_of`/`chunk_count`), never wrap and never own state, so a partial chunk at the torus seam is fine. `chunk_cells` and the ladder are `LodPolicy` config, disabled by default.
  - **Replay identity: hash the policy *only when enabled*; save format v8 writes it unconditionally.** This **diverges from the draft doc's hash-always recommendation** (the doc named this the "clean fallback"). Rationale, ordered by our priorities: a disabled policy produces a byte-identical simulation, so it must produce the identical hash — otherwise two configs that never diverge would get different replay identities (a correctness/determinism defect). Consequences: every existing LOD-off run (physics-only, actual/chains/sandbox packs, all recorded baselines) keeps its exact identity with no churn; an enabled policy is a distinct universe and hashes distinctly, even an all-hot ladder that yields identical particle state. The v8 container always stores the policy so an enabled run resumes into the same universe; v7 saves are not migrated (a Phase 7 concern; no v7 saves are committed). The gate is both-active: pairwise forces, bond springs, and interaction events apply only between two active particles, and integration/decay only to active particles — so conservation of matter/energy holds exactly at every rate (a frozen particle never participates; no one-sided transfer), while trajectory accuracy is the approximation the decision buys.
- **Information overflow cap (2026-07-06, answers Q-2026-07-06-B: Option A, information-only):** clamp information into `[0, information_max]` at interaction commit. `information_max` is a physics param in replay identity (save format bump); default 1e30 — far above any meaningful signal, far below f32 overflow in transfer arithmetic. Matter and energy are conserved by construction and stay uncapped. Amplifying packs saturate instead of detonating to NaN. **Shipped 2026-07-06 (save format v7):** `information_max` is a validated `PhysicsParams` field (finite, > 0, default 1e30), hashed into replay identity; the clamp is applied to both touched particles at interaction commit, and to emitted children at birth. Verified deterministic (fresh/resume/thread-count) on physics-only, sandbox, and hoarders packs.

## Phase 1 — Foundation (done, v0.1.0)

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

## Phase 2 — Physics and Space (done, v0.2.0)

Goal: particles move, collide, and exchange matter/energy under conservation laws, in parallel, deterministically.

Shipped: SoA particle store in canonical (cell, id) order; uniform torus grid; generic short-range kernel (Particle-Life-shaped: repulsion core + attraction band); semi-implicit Euler; rayon chunk-parallel force/integrate passes with thread-count-invariant hashing (proven at 1M particles, see BASELINES.md); matter conserved exactly, momentum/energy within tolerance; save format v2 (physics params in replay identity). Adaptive-detail groundwork deferred to when Phase 3 makes cells meaningfully idle.

Deliverables:

- Spatial partitioning: chunk grid, neighbor queries, particle migration between chunks. Torus wrapping in all position math.
- Motion integration (fixed timestep), collision/force resolution.
- Conservation accounting: total matter and energy tracked per tick; leaks fail tests.
- Multithreaded chunk simulation using the two-phase collect-then-commit rule from the constitution.
- Profiling harness and benchmark suite (criterion or similar) with tracked baselines.
- Adaptive simulation detail groundwork: chunks can run at reduced rates without breaking determinism.

Exit criteria: millions of interacting particles; state hash identical between single-threaded and multithreaded runs of the same seed; conservation tests pass over long runs.

Non-goals: chemistry, bonds, any interaction beyond physical forces.

## Phase 3 — Interaction System and Chemistry (done, v0.3.0)

Goal: the data-driven interaction engine — the heart of emergence.

Shipped: the full interaction action vocabulary — quantity transfers; bonds (canonical id-keyed edge list + per-tick CSR mirror per docs/research/bond-storage.md, harmonic springs, rule-driven create/break); lossy info_copy + physics.information_decay (information deliberately non-conserved); particle emit/absorb (split/merge — emissions append mid-commit, absorptions mark-dead + compact, matter+energy conserved per event, ids never reused; save format v6). All on the RON rule-pack layer with per-pair derived RNG streams. Two-packs-one-engine deliverable shipped: packs/actual.ron (conservation-respecting) vs packs/sandbox.ron (amplifying), same engine, both verified deterministic from the same seed. Exit-criteria review passed 2026-07-06 (docs/research/phase3-exit-review.md): chains.ron produces ~160 persistent multi-particle structures over 20k ticks (oldest keeps identity 19.5k ticks), all packs verify deterministic (fresh/resume/thread-count), conservation exact; f32 info overflow under amplifying content raised as QUESTIONS.md Q-2026-07-06-B. Read-only structure diagnostics live in `genesis run --report N`.

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
