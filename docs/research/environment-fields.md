# Environment fields — design and landing plan (Phase 4)

Status: forks settled in the ROADMAP decisions log (Q-2026-07-08-A);
implementation lands in the staged order below.

The roadmap deliverable: *planet-scale environment fields — temperature,
pressure, gravity, radiation, atmosphere composition — sampled by chunks,
influencing interaction conditions.* This is the world the player will
manipulate in the rest of Phase 4; player tools and replay-recorded edits
build on top of what lands here.

## What a field is

A scalar quantity defined everywhere in space, sampled on its own coarse
torus-aligned grid, that particles can *read* but (for now) not write.
Rules gate on field values the way they already gate on a particle's own
quantities — the environment becomes a spatial condition, so *where* a
structure can emerge is shaped by the world, never scripted.

## Forks and decisions

### F1 — Generic indexed fields, not named engine fields

The constitution names temperature, pressure, radiation as things the
*player* adjusts. Baking those names into engine types would special-case
Earth-like planets (violating "generic systems > Earth-specific logic")
and would fix the field set forever. Instead:

- The engine knows `field[k]` — a config-declared list of scalar fields,
  each with init parameters. Nothing in engine code interprets what a
  field "means"; meaning comes from which rules and (later) physics
  couplings reference it. This is the same shape as the asteroid decision
  (2026-07-06): quantity space, never named substances.
- Config may carry an optional per-field `name` **for humans only** —
  documentation in the RON file, surfaced by UI/Observer later. The
  engine never reads it: it is not hashed and not saved. Two configs
  differing only in names are the same universe.

Vector quantities (a gravity vector) are two scalar fields by convention
when they arrive; no vector machinery now.

### F2 — Own grid resolution, not the interaction grid or LOD chunks

The roadmap sketch said "sampled by chunks", but coupling the env grid to
LOD `chunk_cells` would make a performance-tuning knob part of the
universe's physical shape (and `chunk_cells` is deliberately *not* replay
identity when LOD is off). Coupling it to the interaction grid would make
`interaction_radius` control environment resolution — equally wrong.

- `EnvSpec` declares `cols × rows` for the env grid; all fields share it
  (SoA: one `Vec<f32>` per field). Planet-scale means coarse: tens of
  cells per axis, not thousands.
- Sampling is nearest-cell (`floor(x / cell_w)`, clamped like
  `GridGeom::cell_of`), no interpolation. Interpolation changes replay
  identity and costs 4 taps; it can be a later, deliberate amendment if
  banded emergence looks too blocky.

### F3 — Replay identity only when present; save format v9 writes always

Follows the v8/LOD precedent exactly, for the same correctness reason: a
config with zero fields produces a byte-identical simulation to one that
predates env fields, so it must produce the identical hash. Consequences:

- Zero fields → the snapshot hash gets no env contribution; every
  existing run, pack hash, and baseline keeps its identity.
- One or more fields → grid dims + every cell value of every field enter
  the hash. Fields are *state* (dynamics and player edits will mutate
  them), so the save stores current cell values, not the init spec.
- The init spec itself is **not** hashed and **not** saved: it is fully
  consumed at world creation, like `initial` particle ranges. Two init
  specs that produce identical cell values are the same universe.
- Save format v9 always writes the env block (possibly `field_count: 0`),
  keeping the container self-describing. v8 saves are not migrated
  (Phase 7 concern; none are committed).

### F4 — Rules read the environment at the initiator's position

Each rule gets an optional `env_cond`: a list of `(field, min, max)`
bounds. A candidate pair must satisfy them **at the initiator's env
cell** for the rule to fire.

- One sample per candidate, not two: pair separation is bounded by the
  interaction radius, which in any sane config is far smaller than an env
  cell, so sampling both endpoints doubles cost for no authoring power.
  Both orderings of a pair are evaluated anyway, so a pair straddling an
  env-cell boundary still sees both cells across the two orderings.
- Validation: a rule referencing `field >= field_count` is rejected at
  assembly (same tier as the oversized-radius panic — content that would
  silently misbehave must not reach the hot loop).
- Probability modulation by field value, field-writing actions, and
  per-quantity field coupling are all *later* vocabulary extensions —
  condition gating alone delivers the roadmap criterion (gradients shape
  where Phase 3 structures emerge).

This changes the compiled-rule encoding from a fixed 28-float record to a
variable-length one → save format v10 (landed with this step, separate
from v9 so each step ships format-consistent).

### F5 — Deferred: dynamics and physics coupling

Static fields land first. Field *dynamics* (per-field generic operators:
diffusion across the env grid, decay toward a rest value, sources) and
*physics coupling* (a field pair acting as a force on particles) are
separate work items; their parameters will join replay identity when they
land. Player field-editing tools are the roadmap's next deliverable and
arrive with the replay-recorded action stream.

### F5a — Field dynamics (settled 2026-07-08, Q-2026-07-08-C)

Two generic per-field operators, both optional (default 0 = static field,
bit-identical to before):

- **Diffusion**: explicit-Euler 4-neighbor torus Laplacian in *cell*
  units, `v += diffusion * dt * (Σ neighbors − 4v)`. Conserves the field
  total exactly up to f32 rounding (every flow is antisymmetric).
  Stability requires `diffusion * dt ≤ 0.25` — validated at config load.
- **Relax**: `v += relax_rate * dt * (relax_to − v)` — exponential
  approach to a rest value (a "climate" the field returns to after player
  edits or diffusion disturbances). Requires `relax_rate * dt ≤ 1`.

Scheduling: the env step runs after the player-action drain and before
the particle step, single-threaded over the env grid (a 32×32 grid is
~1k cells — parallelism would buy nothing and cost determinism review).
A field with both rates zero is skipped entirely, so its cells are
untouched bits. Replay identity follows the established rule: dynamics
params hash only when some field has a non-zero rate (an all-static
env keeps its v9 identity); save format v12 stores the three params per
field unconditionally. Sources (constant per-cell injection) are cut
from v1 — `FieldAdd` player actions and `relax_to` cover the authoring
need until something demands them.

## Initial value specs (consumed at creation, never stored)

Enough to author gradients without a noise stack:

```ron
env: (
    cols: 32, rows: 32,
    fields: [
        (name: "warmth", init: Uniform(1.0)),
        (name: "flux",   init: GradientX(lo: 0.0, hi: 4.0)),
        (name: "bands",  init: GradientY(lo: -1.0, hi: 1.0)),
    ],
),
```

`GradientX(lo, hi)` interpolates linearly from `lo` at the west edge to
`hi` at the east edge by env-cell center; `GradientY` likewise north to
south. (The torus seam is a hard step — acceptable for authoring, and a
seeded smooth-noise init can come later if content wants it.) Values must
be finite; cols/rows ≥ 1.

## Landing order

1. **v9 — fields exist.** `EnvSpec` in genesis-config (validated),
   `EnvFields` resource in genesis-sim (grid dims + SoA values +
   nearest-cell sampling), snapshot/hash/save wiring, `from_snapshot`
   restore. Tests: zero-field configs keep pre-env hashes bit-for-bit;
   non-empty fields roundtrip save/load; a differing cell value is a
   different universe; resume matches uninterrupted.
2. **v10 — fields gate rules.** `env_cond` through RuleSpec →
   CompiledRule → hash/save; assembly validation; interact fast path
   (skip env sampling entirely when no rule has env conds). Content:
   `configs/env-gradient.ron` + a pack whose bond-forming rule is gated
   on a field band; a test asserting bonded structures concentrate inside
   the band (the roadmap's "gradients visibly shape emergence" criterion,
   in miniature). Full determinism suite (fresh/resume/thread-count).
3. **Later items** (each its own decision when picked up): field
   dynamics, physics coupling, player edit actions in the replay stream,
   `genesis verify` env mode if dynamics land.

## Performance notes

- Env sampling in the interact pass is one u32 div/mod pair and a bounds
  check per candidate ordering, only when the pack uses env conds — and
  the per-initiator cell can be computed once per particle per tick.
  Bare-physics and env-free packs pay nothing.
- Field storage at planet scale is negligible (32×32 × f32 per field).
- No per-tick work at all while fields are static.
