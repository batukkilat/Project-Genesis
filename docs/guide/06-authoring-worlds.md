# 6. Authoring worlds

A Genesis world is two RON files: a **config** (the planet — its size,
its physics, its environment, who lives there at tick zero) and a
**rule pack** (the chemistry — what can happen when particles meet).
Both are data, never code; both are validated at load; both are part of
replay identity, so editing either creates a new universe rather than
mutating an old one.

Start from the generators, or copy a shipped file that nearly does what
you want (usually faster):

```sh
target/release/genesis init-config my-world.ron
target/release/genesis init-rules my-pack.ron
# then iterate:
target/release/genesis verify --config my-world.ron --rules my-pack.ron --ticks 500
target/release/genesis run --config my-world.ron --rules my-pack.ron --ticks 5000 --report 500
```

Make `verify` a habit: it is cheap, and a world that passes it is a
world you can trust every later observation of.

## The config, knob by knob

```ron
(
    seed: 0,                    // master seed — every RNG stream derives from it
    particle_count: 10000,
    world_width: 1024.0,        // torus: both axes wrap, no edges
    world_height: 1024.0,
    ticks_per_second: 60,       // fixed dt = 1/60 s; warp never changes it
    initial: (                  // uniform ranges the initial population draws from
        matter: (lo: 0.1, hi: 1.0),
        energy: (lo: 0.0, hi: 1.0),
        information: (lo: 0.0, hi: 1.0),
        speed: (lo: 0.0, hi: 2.0),
    ),
    physics: ( /* see below */ ),
    lod: ( /* optional; disabled by default */ ),
    env: ( /* optional; empty by default */ ),
)
```

Every field has a default; a config file only needs the fields it
changes (`( seed: 7 )` is a complete, valid config).

**Physics** — the generic pairwise kernel plus the global quantity laws:

- `interaction_radius` (default 8.0) — force cutoff and spatial grid
  cell size. Also the hard ceiling on any rule's radius.
- `core_frac`, `repulsion`, `attraction` — the kernel's shape: a linear
  repulsion core inside `core_frac × radius`, a triangular attraction
  band outside it. Repulsion alone gives a gas; add attraction and you
  get droplets, lattices, clusters — worlds differ here before any rule
  pack is involved.
- `bond_rest_length` — bonds are springs; this is their rest separation.
- `information_decay` — exponential leak of information per simulated
  second (0 = none). Turn it on and information becomes a *maintained*
  quantity — the entire premise of the selection-pressure packs.
- `information_max` — saturation clamp so amplifying packs can't run to
  overflow. Matter and energy need no cap: they are conserved.
- `spin` — angular velocity of the world frame. Every particle feels the
  Coriolis deflection (applied as an exact, speed-preserving velocity
  rotation — it can never add energy). A spinning world grows coherent
  vortices; the `SpinSet` player action changes this live.

**`lod`** — the adaptive-detail policy (chunk size + activity ladder).
Off by default; see [chapter 10](10-performance-scale.md). The policy is
replay identity when enabled.

**`env`** — declared scalar fields on their own coarse grid:

```ron
env: (
    cols: 32, rows: 32,
    fields: [
        (name: "pressure", init: GradientX(lo: 0.0, hi: 1.0)),
        // init: Uniform(0.5) | GradientX(...) | GradientY(...)
        // optional per-field dynamics: (diffusion: 0.05, relax_rate: 0.01, relax_to: 0.5)
    ],
),
```

Field *names* are documentation only — the engine never reads them;
rules reference fields by index. Init specs are consumed at world
creation; the cell values are state from then on (player actions edit
them, dynamics evolve them). Zero declared fields = exactly the
pre-environment universe.

## Rule packs: condition → probability → action

A pack is a list of rules. Each tick, for every ordered pair of
particles within a rule's `radius`, the rule fires with `probability`
if its conditions hold — the initiator's `self_cond`, the target's
`other_cond`, and the environment's `env_cond` at the initiator's cell.
A fired rule does what its action clauses say:

```ron
(
    rules: [
        (
            radius: 4.0,
            self_cond: ( energy: ( min: 0.3 ) ),          // omitted bounds = unbounded
            other_cond: ( information: ( max: 0.2 ) ),
            env_cond: [ (field: 0, min: 0.6, max: 1.0) ], // omitted = fires anywhere
            probability: 0.05,
            // one or more actions:
            transfer: ( energy: 0.03 ),                   // initiator -> other
            // bond_create: ( strength: 2.0 ),
            // bond_break: true,
            // info_copy: ( cost: 0.02, noise: 0.1 ),
            // emit: ( matter_frac: 0.5, energy_frac: 0.5, info_frac: 0.5, offset: 1.5 ),
            // absorb: true,
        ),
    ],
)
```

The action vocabulary is fixed and small — transfers, bonds, lossy
information copy, emit (split), absorb (merge) — and everything above
it emerges. Matter and energy are conserved through every event; ids
are never reused. The authoring details (what each clause does at the
edges, what aborts an event) are in the
[packs/README.md](../../packs/README.md) authoring notes.

**Learn from the shipped packs, in this order** — each one adds a single
idea, and the README table says what to expect from each:

1. `diffusion.ron` / `hoarders.ron` — one transfer rule; equalizing vs
   amplifying economies from the same vocabulary.
2. `chains.ron` — bonds: the first persistent structures.
3. `echoes.ron` — information copy; with decay on, patterns must be
   re-paid to persist.
4. `churn.ron` — emit/absorb: dynamic population.
5. `actual.ron` vs `sandbox.ron` — full-vocabulary worked pair: the
   same engine, opposite economies.
6. `bands.ron` — `env_cond`: the environment decides *where* chemistry
   happens.
7. `sieve.ron`, `gradient-sieve.ron` — selection pressure: information
   gates survival; then the environment sets how hard the gate bites,
   point by point in space.

## When validation says no

Schema violations fail at load with the field named:

```text
error: invalid config: rule 0: probability must be in [0, 1], got 1.5
```

Cross-file invariants are checked at world assembly and stop the run
with the rule named — the two you will actually meet:

```text
rule 0: radius 100 outside (0, 8] — pairs beyond one grid cell are never scanned
rule 0: env_cond references field 0 but the config declares 0 environment field(s)
```

The first means a rule wants to see farther than the physics grid scans
(raise `physics.interaction_radius` or shrink the rule); the second
means the pack needs an environment the config doesn't declare (add the
field, or run a config that has one).

## Technical notes

- **Replay identity, per knob**: everything in the config and the pack
  changes the universe — seed, counts, world size, tps, initial ranges,
  every physics param (spin and LOD only when enabled/non-zero, by the
  hash-when-active rule), env fields and their dynamics, every rule
  clause. The things that *never* can: observer settings, visual
  mappings/palettes, thread count, report cadence. When in doubt: if it
  could change what happens, it hashes; if it only changes what you see
  or how fast you see it, it can't.
- **Validation is layered**: RON parse errors → schema validation
  (`ConfigError`, clean CLI error) → assembly assertions (cross-file
  invariants, startup panic naming the rule). Nothing invalid ever
  ticks.
- **Probability semantics**: per candidate *ordered* pair per tick,
  rolled on a stream named by (tick, initiator, other, rule) — both
  orderings of a close pair are evaluated independently every tick, and
  thread count can never change a roll.
- **Fractional emit floors**: an emit that would push the parent below
  the minimum matter floor aborts; an info_copy whose energy cost can't
  be paid aborts whole. Rules fail closed — no partial events.
- **Mutation-ready**: the search tooling (`genesis mutate`/`search`)
  jitters, drops, duplicates, and rewires rules within this same schema
  and re-validates after every step — anything you author is a valid
  seed for an evolutionary search (chapter 5).
