# 10. Performance & scale

Genesis targets millions of particles, and reaches them — with the
caveat every simulation shares: how fast a world runs depends on what
the world is *doing*. This chapter is about forming correct
expectations and knowing which knobs trade what.

## What to expect

Rough shapes, not promises (real numbers with machine, tick count, and
exact command live in [BASELINES.md](../../BASELINES.md) — that file is
the authority, this chapter never duplicates its rows):

- **10k–100k particles, physics only**: interactive rates on a laptop;
  this is the scale the shipped example configs use, and where the
  windowed app is comfortable under software rendering.
- **1M particles, physics only**: the Phase 1/2 benchmark scale —
  hundreds of thousands of particle-ticks per second per core, measured
  and tracked since v0.1.
- **10M particles**: runs, and is where the adaptive-detail (LOD)
  policy starts paying for itself (see below).
- **Rule packs change everything.** Interactions cost by candidate
  density and rule count, and worlds that *accumulate* — bonds
  especially — get slower as they run. The committed baselines record
  one pack running 40× slower late in a run than early, in the same
  world, because bonded webs mass-accumulate. When you see a long run
  decelerate, that is usually the world thickening, not the machine
  failing: check the bond count in `--report` before blaming hardware.

## Threads

`--threads K` (0 = all cores) on every heavy subcommand. Two facts:

- Thread count **never changes results** — same state hash at any
  count, which `genesis verify` checks by running single-threaded as
  one of its four legs. Use every core with a clear conscience.
- Scaling is good while the force pass dominates and flattens when
  memory bandwidth or a hot serial phase does; benchmark your own
  machine with `bench --threads` rather than assuming.

## The LOD policy (adaptive detail)

The one performance feature that is also a physics decision. A config
may declare a `LodPolicy`: the world is split into chunks, each chunk's
activity is classified every tick from its state, and cold chunks tick
at a reduced rate from a configured ladder. Trajectories in cold regions
become approximate (that is the point); matter/energy conservation stays
exact at every rate, because interactions only ever apply between two
active particles.

The determinism contract is preserved the strong way: **the policy is
part of replay identity**. Same seed + config + policy = bit-identical
replay, across thread counts and save/resume; a different policy is a
different universe by construction. A disabled policy contributes
nothing — a LOD-off config is the same universe it was before LOD
existed. Machine load, wall clock, and thread count can never influence
classification.

Bench it honestly on one world with the built-in A/B:

```sh
target/release/genesis bench --config configs/lod.ron --particles 3000000 --ticks 40
target/release/genesis bench --config configs/lod.ron --particles 3000000 --ticks 40 --no-lod
```

Expect the win to scale with how much of the world is actually cold
(the baselines record the honest spread: from parity on a hot dense
world to ~2× on a settling one).

## Time warp is CPU-bound, and "starved" is honesty

In the app, warp presets ask for a multiple of real time. The pacer
plans whole ticks per frame from measured frame time; when the CPU
cannot deliver the requested rate, the HUD shows target vs achieved and
a **starved** flag. Nothing is skipped or approximated to fake the
rate — dt never changes, ticks are whole, and a starved warp simply
runs slower than asked. Headless runs have no pacer: they always tick
flat out.

## Software rendering (WSLg / no GPU)

Under WSLg the only Vulkan adapter is llvmpipe — the app works but
renders on the CPU, competing with the simulation. The app caps itself
at 30 fps there (`--fps` to change); rendering detail scales
independently of simulation state, and the T2/T3 heatmap tiers are
cheap by design (quarter-resolution raster, integer upscale). On any
real GPU the renderer is not the bottleneck at current scales.

## Technical notes

- **Benchmark discipline**: every baseline row records machine, tick
  count, and command, because throughput *and the state hash* depend on
  all three — a hash without its tick count is unusable (a lesson
  BASELINES.md records from experience). Compare like with like:
  fresh-start ticks and late-run ticks of an accumulating pack are
  different regimes of the same world.
- **Why accumulating packs decelerate**: interaction cost scales with
  candidates per neighborhood walk; bonds add spring forces and
  ever-denser candidate sets. Bond count is the single best cheap
  predictor of wall time — the search tooling's circuit breaker is a
  bond cap for exactly this reason.
- **Where time goes at scale** (from the committed profiling): the
  pairwise force pass dominates dense worlds; the canonical (cell, id)
  re-sort is incremental and scales with *motion*, so settled worlds
  skip it; interaction collect walks each neighborhood once regardless
  of rule count.
- **Memory**: the particle store is SoA and compact, but 10M+ particles
  with bonds is real memory — worlds beyond RAM (chunk streaming) are
  deliberately deferred until an out-of-memory scale target exists
  (docs/research/chunk-streaming.md).
