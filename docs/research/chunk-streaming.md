# Chunk streaming / persistence — scope and timing (Phase 4)

Status: scoped 2026-07-08; **implementation deliberately deferred** until a
concrete out-of-memory scale target exists. This doc records why, and the
invariants any future implementation must hold, so the decision is a
recorded one rather than a gap.

## The deliverable

ROADMAP Phase 4: *"Chunk streaming / persistence so planets exceed
memory."*

## Why not now

- **No memory pressure exists.** The 10M-particle baseline world uses on
  the order of half a GB for particle state; the engine's current scale
  ceiling is *throughput* (BASELINES.md: ~1.4–1.8e6 particle-ticks/s at
  10M), not residency. A planet big enough to exceed RAM would tick so
  slowly today that streaming it would solve the wrong problem first.
- **The known perf follow-up comes first.** The per-tick `canonicalize`
  sort is the recorded bottleneck that bounds LOD's speedup (BASELINES.md,
  Phase 4 notes). An incremental/bucketed re-sort that skips frozen
  regions raises the scale ceiling directly and reshapes the residency
  question (a sorted-by-cell store is also the natural streaming layout).
  Sequencing streaming *after* that work avoids designing against a
  layout that is about to change.
- Priorities: correctness and determinism work is available elsewhere
  (Phase 5 Observer can begin — Phase 4's exit criteria pass), and
  premature streaming machinery is exactly the "architecture to stay
  busy" GOAL.md warns about.

## Invariants for whenever it lands

1. **Streaming is caching, never state.** Where a chunk's particles live
   (RAM vs disk) must be invisible to replay identity: same seed + config
   + actions = bit-identical hashes with streaming on, off, or thrashing.
   Eviction may be driven by machine-local memory pressure *only because*
   residency can never influence a single simulated bit — the analogue of
   thread count today.
2. **Residency must cover every read the tick makes.** The force pass
   reads *frozen* neighbors' cell membership (to skip them) and active
   neighbors' positions; any page-out unit therefore needs its border
   region resident whenever an adjacent active region ticks. The natural
   page unit is the LOD chunk (already the freeze unit): a chunk is
   evictable only while every chunk in its 3×3 neighborhood is frozen.
3. **The on-disk page format is the save format's particle record** (id,
   pos, vel, matter, energy, information — already canonical, id-sorted
   within (cell, id) order), so streaming and save/load share one
   serialization and one integrity story.
4. **Bonds pin residency.** A bond spring reads its partner's position
   whenever either endpoint is active; a chunk containing an endpoint of
   a bond into an active chunk is not evictable. (The CSR mirror already
   knows this adjacency.)

## Trigger to revisit

Set a scale target that does not fit the reference machine's RAM
(≈100M+ particles, or environment/planet data of comparable size), or
Phase 6's zoomed-out planet view demanding worlds of that size. At that
point: write the design against the then-current store layout, with the
incremental-sort work already landed.
