# 1. What Genesis is

Project Genesis is a sandbox where you set up a small universe and watch
what it does. Not a universe of creatures, plants, or civilizations — a
universe of **particles**: little packets of matter, energy, and
information drifting on a world that wraps around at the edges. Particles
push and pull on each other, collide, bond into larger assemblies, trade
what they carry, split, and merge. That's the entire cast. Everything
else you will ever see in Genesis — clusters that persist for thousands
of ticks, webs that grow, patterns that hold themselves together against
decay — is something the particles *did*, not something the program
contains.

That is the core promise, and it cuts both ways:

- **The engine never knows.** There is no code for "life", "species",
  "organism", or "intelligence" anywhere in the simulation. If a blob of
  bonded particles maintains itself for an hour, the engine did not
  arrange that; it only ever moved quantities around according to the
  rules you loaded.
- **You shape environments, never creatures.** Your verbs are a planet's
  verbs: paint a field warmer or colder, spin the world, throw an
  asteroid, crack a rift, change how fast time runs. There is no tool
  that touches an individual particle, and there never will be. You are
  the climate, not the hand of god.
- **Every outcome is valid.** Nothing in Genesis wins or loses. A world
  that collapses into dust is as legitimate a history as one that fills
  with structure. The game loop is curiosity: set conditions, run,
  observe, fork, compare.

## The layers

Genesis is built as a strict stack — each layer reads the one below it
and can never write back down:

```
  AI narration        (planned)   tells the story of a run
  Rendering / app     genesis-app watches the world, sends only player actions
  Observer            genesis-observer   detects structures, guesses at what they are
  Simulation          particles + physics + interaction rules — the world itself
```

The simulation runs complete and deterministic with nothing above it
attached. The **Observer** is a scientist looking through glass: it finds
structures in the bond graph, tracks them over time, and ventures
carefully hedged hypotheses ("possibly self-maintaining"), but it is
physically unable to change a single simulated bit — see
[chapter 9](09-observer.md). The **app** is a window with controls;
everything you do in it becomes an ordinary, replayable player action.
AI, when it arrives in Phase 7, will only narrate what the Observer
already recorded.

## What "deterministic sandbox" buys you

Genesis is deterministic in a strong, tested sense: the same version,
seed, configuration, rule pack, and player actions produce the same
universe, tick for tick, bit for bit — on one machine, across thread
counts, and across save/resume. This is why:

- **You can reproduce anything.** A world is fully named by its seed +
  config + rules + action script. There is no "I saw something amazing
  once and can't find it again" — the run *is* those four things.
- **You can share worlds as recipes**, not gigabytes: send someone your
  config and script and they will grow the identical history.
- **You can fork timelines.** Save at tick 50 000, branch, and run two
  futures of the same past — the untouched branch stays bit-identical to
  its parent, so any difference you observe is exactly what you changed
  and nothing else. Counterfactuals become experiments.

## Technical notes

- The constitution — immutable rules the project is built under — is
  [Prompts/MASTER_PROMPT.md](../../Prompts/MASTER_PROMPT.md); the phased
  plan and decision history live in [ROADMAP.md](../../ROADMAP.md).
- Space is a 2D torus (continuous `f32` positions, both axes wrap). Time
  is a fixed timestep; time warp runs *more ticks per wall second* and
  never changes the step size, so warping can never alter outcomes.
- A particle carries: id, position, velocity, matter, energy,
  information, bonds, state. Matter and energy are conserved through
  every interaction event; information is deliberately not (it can be
  copied at an energy cost and decays if configured) — it behaves like
  signal, not substance.
- Interactions are data, not code: RON rule packs of
  condition → probability → costs → action, validated at load and hashed
  into replay identity — a different pack is a different universe by
  construction. See the shipped packs in [packs/](../../packs/) with
  their README table.
- Determinism scope is same-platform, same-build. Cross-platform
  bit-exactness is explicitly a non-goal.
- "Deterministic" is enforced, not hoped: `genesis verify` runs a world
  four ways (twice fresh, once through save/resume, once single-threaded)
  and requires identical state hashes; the test suite does the same
  around every feature that could touch replay identity.
