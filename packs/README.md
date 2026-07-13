# Rule packs

Authored interaction rule packs (RON). A pack is content, not code: it
compiles at load into the engine's internal rules and is hashed into replay
identity — same seed + config + pack = same universe, a different pack is a
different universe.

## Two packs, one engine

`actual.ron` and `sandbox.ron` are the Phase 3 deliverable "Actual Physics
and Sandbox Physics as two rule packs on the same engine, not two code
paths." Both use every action in the vocabulary (transfers, bonds,
info-copy, emit/absorb); both are bit-deterministic; the engine conserves
matter and energy through every event in both. The difference is pure
content: in `actual` flows equalize and everything costs something, in
`sandbox` flows amplify and everything is cheap. Run them from the same
seed and watch two different universes:

```sh
genesis verify --config demo.ron --rules packs/actual.ron  --ticks 500
genesis verify --config demo.ron --rules packs/sandbox.ron --ticks 500
```

Usage:

```sh
genesis run    --rules packs/diffusion.ron --ticks 5000
genesis verify --rules packs/hoarders.ron --ticks 1000
genesis init-rules my-pack.ron   # writes a starter pack to edit
```

| Pack | Regime | Expected long-run shape |
|---|---|---|
| `diffusion.ron` | equalizing — energy flows rich→poor, matter drifts light→heavy slowly | gradients flatten; quantities spread |
| `hoarders.ron` | amplifying — energy flows poor→rich, matter light→heavy | inequality concentrates; hoards emerge (or churn — seed-dependent) |
| `chains.ron` | binding — energetic close pairs bond into springs, rare spontaneous breaks | multi-particle structures that persist against churn (chains, rings, blobs — seed-dependent) |
| `echoes.ron` | imprinting — info-rich particles copy information onto blank neighbors at an energy cost | information fronts spread and compete; with `information_decay` on, only re-imprinted patterns persist |
| `churn.ron` | dynamic population — energetic heavies split, heavies eat dust | particle count finds its own balance; matter/energy conserved through every split and merge |
| `actual.ron` | conservation-respecting regime — flows equalize, structure costs, info is expensive | slow condensation: bonded clusters where energy concentrates, patterns fade unless re-paid |
| `sandbox.ron` | anything-goes regime — flows amplify, bonds and copies are free | runaway concentration and info saturation; boiling population |
| `bands.ron` | environment-gated — bonding only works where env field 0 is mid-range; breaks happen everywhere | structures accumulate only inside the band (pair with `configs/env-gradient.ron`); the environment shapes *where*, never *what* |
| `sieve.ron` | selection-pressure — information gates survival (info-poor get absorbed), reproduction (only info-rich split), and membership (bonds form between the info-rich, break to the info-poor); config decay makes information a maintained quantity | clusters made of maintained information: what keeps its information above the floor persists, everything else is recycled (pair with `configs/sieve.ron`) |

Authoring notes:

- Omitted condition bounds mean unbounded; omitted transfers mean zero.
- `radius` must not exceed the physics `interaction_radius` (the grid cell) —
  the simulation refuses oversized rules at startup.
- `probability` is per candidate ordered pair per tick, rolled on a stream
  named by (tick, initiator id, other id, rule) — thread count can never
  change outcomes.
- `bond_create: ( strength: k )` bonds the pair with a spring of stiffness
  `k` and rest length `physics.bond_rest_length`; creating an existing bond
  is a no-op (bonds never stack). `bond_break: true` removes the pair's bond
  if present. A rule may do one or the other, not both.
- `info_copy: ( cost: c, noise: n )` overwrites the other particle's
  information with the initiator's value degraded by up to ±n (fraction,
  0..1). The initiator pays `c` energy to the receiver; if it cannot pay,
  the entire event aborts, transfers included. Information is deliberately
  not conserved — copies create it, `physics.information_decay` destroys it.
- `emit: ( matter_frac, energy_frac, info_frac, offset )` spawns one child
  from the initiator's stocks (fractions are moved, so every quantity is
  conserved). The child inherits the parent's velocity and appears `offset`
  units away; the event aborts if the matter split would break the mass
  floor. `absorb: true` destroys the other particle, moving all its stocks
  to the initiator (mass-weighted velocity merge). Emit and absorb are
  mutually exclusive per rule; ids are never reused.
- `env_cond: [ (field: k, min: a, max: b), ... ]` gates the rule on the
  config's environment fields, sampled at the initiator's env cell — every
  listed bound must hold or the rule never fires there. Omitted ends are
  unbounded; omitted `env_cond` fires anywhere. Field indices refer to the
  config's `env.fields` list and are validated at startup: a pack that
  references field `k` needs a config declaring at least `k + 1` fields.
- No biology, no win conditions. Packs describe how quantities move; whatever
  structure appears, emerges.
