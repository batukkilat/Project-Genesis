# Renderer bootstrap — Phase 6 landing plan

Status: decision recorded in the ROADMAP decisions log (Q-2026-07-09-B);
implementation lands in the staged order below. **Step 1 landed
2026-07-09** (`genesis-render`, extraction core, Bevy-free, headless-tested). Companion to
docs/design/visuals.md (the look) and docs/design/ui.md (the chrome) —
this file is the architecture: how pixels get fed without touching the
simulation.

Constitution constraints: renderer never owns simulation logic; simulation
crates must not depend on Bevy's rendering features; the sim runs fully
headless with the renderer absent; the player modifies environments only —
the UI emits replay-recorded `PlayerAction`s, never state edits.

## The fork: how the renderer reads state (open design question 4)

The question deferred since Phase 5 ("double-buffering vs copy-on-write"),
now due. Options considered:

- **A — lockstep, extract-per-frame.** One thread. The app owns the
  `Simulation`, ticks it inside the frame loop (0 ticks when paused, k
  ticks per frame under warp), then runs one *extraction pass* that turns
  sim state into a `RenderFrame` — plain data (sprite instances or cell
  aggregates per LOD tier) with no references into the sim. Rendering
  consumes only `RenderFrame`.
- **B — sim thread + double-buffered snapshots.** The sim ticks on its own
  thread at its own rate and publishes into a double buffer; the render
  thread reads the latest published frame. True render/sim rate
  independence.
- **C — copy-on-write world.** Structural sharing so snapshots are O(1).
  Heavy machinery, foreign to the SoA store design.

**Decision: A for v1, with B's interface.** The seam is `RenderFrame`:
because rendering already consumes extracted plain data rather than sim
references, promoting extraction onto a sim thread later (B) changes no
consumer code — only who calls it and where the buffer lives. A is the
Observer precedent (read-only consumption of state between ticks, zero-copy
deferred until profiling demands it), is trivially correct (no torn reads —
the sim never mutates while extraction runs), and keeps determinism
reasoning untouched: ticks happen in one loop, wall clock never enters the
simulation. The spec's "simulation speed independent" requirement is
satisfied honestly at v1 scale by extraction being cheap (one pass over the
store / one aggregation over `cell_start`), and structurally later by the
seam. C is rejected outright — it buys nothing A+B don't and taxes the SoA
layout that Phase 2 performance is built on.

Two sub-decisions that follow:

- **Warp under load:** the frame loop runs as many ticks as the warp preset
  asks, bounded by a per-frame wall budget; the UI shows target vs achieved
  ticks/s (ui.md honesty principle). Time warp remains "more ticks per wall
  second, never a bigger dt", and the wall clock still never reaches the
  sim — it only decides *how many* whole ticks this frame runs, which is a
  presentation concern (same seed + actions ⇒ same states regardless).
- **Interpolation:** none in v1. Render the latest state (visuals.md's
  leaning). Interpolating between two ticks needs a second retained frame —
  add it only if 1× motion visibly stutters.

## Crate: `genesis-render`

- Depends on `genesis-sim`, `genesis-config`, `genesis-observer` (panel
  data), `genesis-persist` (save/load UI), and full Bevy. Nothing below it
  changes: sim crates keep depending on `bevy_ecs` only (workspace
  already pins the version — the render crate must use the same Bevy
  release).
- The headless binary (`genesis-headless`) keeps zero renderer deps; a new
  `genesis-app` binary (or a bin target inside `genesis-render`) hosts the
  windowed app. CI keeps building/testing the whole workspace; the render
  crate's logic (extraction, LOD tiering, mapping) is unit-tested headless —
  only window/GPU smoke tests need hardware and are dev-machine-only.

## `RenderFrame` extraction (the testable core)

Pure functions from `(&Simulation state, camera rect, zoom tier)` to plain
data, following visuals.md tiers:

- **T0/T1:** visible-particle instance list (position, radius from matter,
  brightness from energy, hue from information) + bond segments (T0 only).
  Torus-aware: a camera rect crossing the seam yields duplicated instances
  at wrapped positions.
- **T2/T3:** per-cell aggregates (count, mean energy, mean information),
  plus env-field values underneath (T3). *As landed:* aggregation covers
  the whole world every frame, not just the camera rect — one linear pass
  over the store is exactly the v1 extraction budget this plan accepts,
  and the raster layer crops via its world-rect sampling. Rect-scoped
  aggregation (walking `cell_start` for visible cells only) is a
  profiling-driven refinement, not a correctness change.
- Mappings (quantity → radius/brightness/hue, palettes) are RON data files,
  hot-swappable, never replay identity (visuals.md principle 4).

Extraction is where all determinism-adjacent care lives: it *reads* the
canonical layout but must never mutate the store (take `&Simulation`;
the existing `snapshot()` path stays the save/hash truth). Unit tests:
torus seam duplication, tier thresholds, aggregate math, mapping edges.

## Landing order

1. **Crate + extraction core, no window.** `genesis-render` with
   `RenderFrame`, tier selection, T0–T3 extraction, RON mapping loader.
   Fully unit-tested headless. (This step is most of the correctness
   surface and needs no GPU.)
2. **Bevy app shell.** Window, camera pan/zoom (bounded), frame loop
   ticking the owned `Simulation`, T0 sprites from `RenderFrame` via one
   soft-dot texture. Debug overlays first (visuals.md: they de-risk the
   mapping pipeline before aesthetics).
3. **Heatmap tiers + pixel look.** T2/T3 textures from aggregates,
   low-res offscreen target + integer upscale + dither. *Logic half landed
   2026-07-09* (`raster` module): RON palette ramps (`palettes/`),
   aggregate→RGBA8 rasterization with torus-wrapped world-rect sampling
   and 4×4 Bayer ordered dithering — the Bevy half only uploads the buffer
   and upscales.
4. **Time controls + player actions.** pause/1×/warp presets with
   target-vs-achieved display; first env tool (field brush) emitting
   `PlayerAction`s through the exact scripted-action path — the UI is just
   another script author (Q-2026-07-08-B: one representation). *Logic half
   landed 2026-07-09* (`pacer` module): `WarpPacer` turns measured frame
   time + a tick budget into a whole-tick `FramePlan` with a starvation
   flag for the honesty display; fractional-tick carry, no catch-up
   bursts, wall clock never enters the sim.
5. **Observer panel + inspector,** panel-only annotations (owner decision
   2026-07-08). Save/load/branch UI last.

Steps 1 and the logic halves of 3–5 are verifiable on a headless box;
steps 2's window and visual polish need a dev machine with a display —
autonomous cloud sessions should stop at the testable boundary and leave
window-runtime verification to the owner or a desktop session.

## Non-goals (v1)

Chunk streaming visuals (sim side is deferred anyway), audio, AI narration
hooks (Phase 7), interpolation, multi-viewport, and any Observer overlay
drawn on world objects (settled: never).
