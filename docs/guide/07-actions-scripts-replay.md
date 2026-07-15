# 7. Player actions, scripts, replay

Everything a player can do to a Genesis world is an **action**: a small,
tick-stamped data record. Paint a field in the app — that's an action.
Write the same edit in a script file — the identical action. Drop an
asteroid, crack a rift, spin the planet: actions. This is deliberate and
absolute — there is exactly one representation, so a live play session,
a recorded one, and a hand-written experiment script are the same kind
of thing and replay the same way.

## The vocabulary

The verbs are a planet's verbs, never an organism's:

- **`FieldSet` / `FieldAdd`** — set or offset an environment field over
  an axis-aligned world-coordinate region. The brush in the app emits
  exactly these.
- **`Impact`** — an asteroid: a radial momentum shock, an energy
  deposit split by falloff, and a particle payload drawn from declared
  quantity ranges (matter/energy/information/speed — a region of
  quantity space, never a named substance). The one way matter enters a
  world from outside, and the injection is exactly what the action
  declares.
- **`Rift`** — a tectonic event: the same shock shaped along a world
  segment (perpendicular impulse, payload upwelling along the line). A
  zero-length rift is bitwise an impact.
- **`SpinSet`** — set the world frame's angular velocity from this tick
  on (see the spin knob in [chapter 6](06-authoring-worlds.md)).

Time warp is *not* an action and never will be — it cannot affect state,
so recording it would be noise.

## Scripts

A script is a RON list of stamped actions, applied at the start of their
tick in script order:

```ron
(
    actions: [
        (
            tick: 2000,
            action: FieldSet(
                field: 0,
                region: (x0: 0.0, y0: 0.0, x1: 512.0, y1: 1024.0),
                value: 0.6,
            ),
        ),
    ],
)
```

Feed one to any headless run or verify with `--actions`. The shipped
scripts are worked examples, each documented in-file, each passing the
four-way determinism check on the current build:

```sh
target/release/genesis verify --actions scripts/spin-up.ron --ticks 4000
target/release/genesis verify --actions scripts/bombardment.ron --ticks 2000
target/release/genesis verify --actions scripts/rift.ron --ticks 2000
target/release/genesis verify --config configs/env-gradient.ron --rules packs/bands.ron \
    --actions scripts/terraform-west.ron --ticks 3000
target/release/genesis verify --config configs/full-stack.ron --rules packs/bands.ron \
    --actions scripts/full-stack.ron --ticks 3000
```

- **terraform-west** — the Phase 4 exit criterion, executable: on the
  gradient world, mid-run, it terraforms the west half into the band
  where `bands.ron` chemistry works; structure starts growing where the
  player made room for it.
- **spin-up** — three eras in one run: spin up at tick 200, reverse at
  2 000, stop at 3 800.
- **bombardment** — a heavy central strike, then a light glancing one.
- **rift** — segment-shaped shocks with upwelling payloads.
- **full-stack** — everything at once (spin + field edits + impact +
  rift on a dynamic-field, LOD-enabled world); exists precisely to run
  every feature in one deterministic scenario.

## Sharing and re-running a session

A run is fully named by **seed + config + rules + actions** (all four
are replay identity). Send someone those files and they grow your exact
universe:

```sh
target/release/genesis run --config configs/env-gradient.ron --rules packs/bands.ron \
    --actions scripts/terraform-west.ron --ticks 5000 --hash-every 1000
```

Identical hashes at every step, on any machine of the same platform and
build. Live app sessions are the same story: every brush stroke goes
through the same queue the script loader uses and is appended to the
branch's action log, so a played session *is* a script you can re-run
(chapter 8 covers the logs and forking).

## Technical notes

- **Scheduling**: actions apply at the *start* of their stamped tick,
  in stable script order for same-tick actions. Past-stamped actions
  are rejected at load — a script can't rewrite history.
- **Replay identity**: pending actions hash (with every parameter);
  applied actions don't need to — their effects are already state. An
  action-free run keeps the exact identity it had before the action
  system existed.
- **Mid-script saves work**: the pending queue rides in the save, so a
  world saved between two scripted actions resumes into the identical
  future. `verify --actions` deliberately saves mid-script as one of
  its legs.
- **Determinism of payloads**: impact/rift payload draws use an
  order-free stream derived from (tick, queue position), so same-tick
  events draw independently and scripted vs live delivery is
  bit-identical.
- **Seam wrapping**: a region crossing the torus seam is authored as
  two rects (the app's brush does this for you); a rift segment may
  cross the seam but must not span more than one world period per axis
  — assembly rejects it.
- **Validation**: scripts pass the same structural validation live
  actions do (one acceptance boundary); a malformed region, negative
  radius, or unknown field index is a load error naming the action.
