# 8. Saves, forks, timelines

A Genesis save is a complete world in a file: every particle, every
bond, the rules, the physics, the environment, and anything still
pending in the action queue. Load it and the universe continues as if it
had never stopped — not approximately, but bit for bit: a run that
saves, loads, and continues produces the *identical* state hash to one
that never paused. That guarantee is what makes saves research
equipment rather than checkpoints.

## Saving and resuming

```sh
target/release/genesis run --rules packs/chains.ron --ticks 5000 --save world.gens
target/release/genesis run --load world.gens --ticks 5000
```

The second command continues to tick 10 000. You can save mid-anything —
a pending scripted action, an active spin, an LOD policy mid-stride —
and the resumed run picks it all up; the test suite deliberately saves
in the middle of every such effect. One rule: `--load` refuses
`--rules`, because the rule set is part of the saved world's identity —
you cannot swap chemistry into an existing universe.

In the windowed app the save/load UI arrives with the panel work
(chapter 4); until then, headless saves and the app read the same
`.gens` files.

## Forking a timeline

```sh
target/release/genesis branch --from world.gens --to fork.gens
```

This copies the world and writes an ancestry sidecar next to it
(`fork.gens.branch.ron` unless `--record` chooses another path):

```ron
(
    format: 1,
    parent: Some((
        save: "world.gens",
        state_hash: 16861098152697180637,
        tick: 300,
    )),
    actions: [],
)
```

The child starts as the parent's exact state — the branch point is
named by the parent's tick and state hash — and an empty action log.
From here the two futures are independent worlds: run the parent
untouched and it stays bit-identical to what it always was; act on the
fork and every divergence is exactly what you did, nothing else. Fork
several children from one parent and they share ancestry through the
same chain.

A branch must be a new file — `branch` refuses to overwrite an existing
save or record (overwriting would silently destroy that branch's state
and history), and refuses `--from X --to X` for the same reason.

## The fork-and-compare workflow

The loop these pieces exist for:

1. Run a world overnight, headless, saving at the end (or at interesting
   ticks with intermediate `run --load`/`--save` legs).
2. In the morning, `branch` it into as many children as you have
   questions: one gets an asteroid script, one gets a field edit, one
   runs untouched as the control.
3. Run each child the same number of ticks and compare — by eye in the
   app, or by `--report`/`--timeline`/`score` (chapters 9 and 5).

Because the control child is bit-identical to "what would have happened
anyway", any difference you measure is caused by your intervention. This
is a counterfactual experiment on a universe — the thing determinism was
built to buy.

## Technical notes

- **Format**: `.gens` is a hand-rolled versioned binary (magic `GENS`),
  no serialization library in the format. The version is checked at
  load; the file also stores the world's state hash, so corruption or a
  dropped field surfaces as a load error, never as a silently different
  world.
- **What's inside**: format version, tick, RNG state and stream seed,
  physics params (including spin), LOD policy, env field values and
  dynamics params, every particle and bond, the full compiled rule set
  (which is why `--load` needs no `--rules`), and the pending
  player-action queue. Save format changes are one-way: new engine
  versions read their own format (migration between versions is a
  Phase 7 deliverable).
- **Ancestry lives above the engine.** The branch record is RON
  bookkeeping the engine never reads; nothing about ancestry enters
  replay identity. Deleting a record loses history, not correctness.
- **Action logs**: the UI and scripted runs append every emitted action
  to the branch's log (a scripted run's log is its script), so a chain
  of records reconstructs how any timeline was produced. Packaging a
  whole chain into one shareable replay file is a Phase 7 deliverable.
- **Saves are not scores**: Observer output (timelines, scores) is never
  stored in `.gens` — recompute it from the world at whatever cadence a
  question needs.
