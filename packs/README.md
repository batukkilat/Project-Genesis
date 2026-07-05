# Rule packs

Authored interaction rule packs (RON). A pack is content, not code: it
compiles at load into the engine's internal rules and is hashed into replay
identity — same seed + config + pack = same universe, a different pack is a
different universe.

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
- No biology, no win conditions. Packs describe how quantities move; whatever
  structure appears, emerges.
