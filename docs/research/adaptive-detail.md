# Adaptive Simulation Detail (LOD) — Phase 4 groundwork design

The first Phase 4 work item (ROADMAP decisions log, 2026-07-06). Settled by
Q-2026-07-06-A (Option A): **approximate LOD, with the detail policy part of
replay identity**. Chunks are classified by a state-keyed activity metric;
cold chunks run every k-th tick; the policy (metric, thresholds, rate ladder)
is configuration, hashed like physics params — same seed + config + policy =
bit-identical replay; a different policy is a different universe.

This document turns that settled decision into a concrete, code-grounded
implementation plan. It is a **proposal**: the implementer ratifies the forks
marked *Decision* below into the ROADMAP decisions log (with Q ids) in the
landing commit, per the standing owner guidance. Nothing here is settled yet.

## TL;DR — Recommendation

Model a "cold" chunk as a region that **does not exchange anything with the
rest of the world on skipped ticks** — the cleanest route to the hard
conservation invariant. Concretely:

- **Chunk = a `chunk_cells × chunk_cells` block of existing grid cells.** Pure
  geometry on `GridGeom`, derived from a particle's cell, recomputed every tick
  like `cell_of`. No new persistent objects, no history dependence.
- **Per-tick, stateless classification.** Each tick, compute a per-chunk
  activity metric by an *order-independent* reduction (max over the chunk's
  particles of a non-negative per-particle scalar), map it through a fixed
  threshold→rate ladder, and mark a particle *active this tick* iff
  `tick % rate(chunk) == 0`. Rates are a pure function of `(state, policy,
  tick)` — no saved LOD state, no hysteresis in the first cut.
- **"Both-active" gating.** A pairwise force, a bond spring, or an interaction
  event applies **iff both particles are active this tick.** Integration
  (position/velocity/info-decay) applies to active particles only; inactive
  particles are frozen bit-for-bit.
- **Bounded max rate.** The ladder's slowest rate is finite (config), so every
  chunk runs at least every `max_rate` ticks — no chunk is ever permanently
  frozen, so a quiet region can always be re-woken by a later active tick.

Why this shape: conservation falls out *by construction*. A pairwise force is
applied to two active particles with `torus::delta` antisymmetry → equal and
opposite → momentum exact. An interaction event fires only between two active
particles and conserves matter/energy per event exactly as today. Anything a
frozen particle would have exchanged simply does not happen — no half-applied
impulse, no one-sided transfer, nothing to leak. The approximation is entirely
in the *dynamics* (frozen regions don't feel their neighbors for up to
`rate` ticks), which is exactly the "approximate LOD" Q-2026-07-06-A permits.

## What "conservation holds at every rate" means here (important)

The hard invariant (Q-2026-07-06-A, item 2) is about the **fundamental
quantity stocks** — total matter and total energy — not kinetic energy.

- **Matter and energy stocks:** conserved *exactly* (to f32 rounding, as
  today) because only interaction events move them, and events fire only
  between two active particles, each event already conserving per the
  constitution. LOD never creates a partial or one-sided transfer.
- **Kinetic energy / trajectory accuracy:** *approximated*. Skipping force
  impulses between a hot particle and its frozen neighbor changes trajectories.
  Symplectic Euler already only bounds (never exactly conserves) kinetic+
  potential energy; LOD widens that bound in exchange for skipped work. This
  is the trade the decision explicitly buys, and it is invisible where it is
  used — cold regions are cold precisely because little is moving.

A cross-rate **conservation test** therefore asserts that total matter and
total energy stocks are constant across a run where chunks straddle rates and
an emit/absorb pack is active — never that kinetic energy is unchanged.

## Where each gate lands in the current loop

`sim_step` (crates/genesis-sim/src/lib.rs) today is:

```
canonicalize → bonds.rebuild → forces → interact::apply → integrate
```

The activity mask is computed once, right after `canonicalize` (so it sees the
tick's canonical layout and current state), and is then read by the three
passes:

1. **Classify** (new, O(n)): per-particle activity scalar → per-chunk max →
   per-chunk rate → per-particle `active[i]` bool. Parallel; `max` reductions
   are order-independent so thread count cannot change the result. Cheap
   relative to the force pass it gates.
2. **`forces`** (physics.rs): for an active `i`, accumulate the kernel force
   only from active `j`; skip inactive `i` entirely. Same both-active rule for
   bond springs. (A bond straddling a rate boundary is paused while either end
   is cold — conserves; note it, it is an edge case since bonded pairs are
   usually co-located and share a chunk.)
3. **`interact::apply`** (interact.rs): in Phase A (collect), skip a candidate
   pair unless both endpoints are active. Everything downstream is unchanged;
   `information_max` clamping and emit/absorb conservation are untouched.
4. **`integrate`** (physics.rs): update position/velocity and apply info decay
   only for active particles; frozen particles keep their exact state.

Emit children inherit the initiator's chunk (they spawn at `emit_offset`,
well within a chunk) and are active on their birth tick because their parent
was active to emit them — no special case.

## The classification metric

Per-particle activity scalar must be **non-negative and finite** (so `max` is
a clean, order-independent reduction — the `information_max` clamp already
rules out the NaN that would break `max`). A workable first metric:

```
activity(i) = vx² + vy²            // speed²; cheap, already in the store
```

Optionally add `+ (fx² + fy²) * w_force` once forces are known — but forces are
computed *after* classification in the current order, so the first cut uses
speed² alone (available straight from `canonicalize`). A chunk's metric is the
max activity over its particles; an empty chunk has metric 0 and takes the
coldest rate.

The **rate ladder** is a short, validated, monotone table, e.g.

```
[ (threshold_0, rate_0=1), (threshold_1, rate_1), … ]   // rate_0 == 1 (hot)
```

read as "metric ≥ threshold_k selects rate_k"; the coldest bucket caps at
`max_rate`. Constraints (config validation): all thresholds finite and
ascending; all rates ≥ 1; `rate_0 == 1`; `max_rate` finite and ≥ 1.

*Decision (fork, ratify on landing):* **per-tick stateless classification**
(no hysteresis) for the groundwork. Rationale: it needs zero saved LOD state,
so save/resume is bit-identical for free (the mask recomputes from restored
state), and it cannot desync a resume. Its cost is possible rate *thrash* at a
threshold boundary, which affects only performance, never correctness or
determinism. Hysteresis (a saved previous-rate array + a format bump) is a
later refinement if profiling shows thrash matters.

## Determinism and save/resume

- The mask is a pure function of `(state, policy, tick)`. `state` and `tick`
  survive save/resume; `policy` is in replay identity. So a resumed run
  recomputes the identical mask on its first tick — no LOD state is saved.
- `max`-based per-chunk reduction is order-independent → thread-count
  invariant, matching the existing "thread count cannot change a bit" property.
- Frozen particles don't move, so their cell (and chunk) is unchanged by the
  next `canonicalize`; the layout stays a pure function of state.

## Config surface and replay identity

Add `LodPolicy` to `SimConfig` (RON, `#[serde(default)]`), `enabled: false` by
default. Fields: `enabled`, `chunk_cells`, the rate ladder, `max_rate`.

*Decision (fork, ratify on landing):* **always hash the policy** into the state
hash and **bump the save format** (v7 → v8), with `enabled: false` a distinct,
trivially-encoded policy value. This is the straightforward reading of
Q-2026-07-06-A ("hashed into replay identity like physics params") and avoids
the bug surface of conditional hashing. Consequence: existing pack goldens
(chains/actual/sandbox) change hash once when LOD lands — expected for a new
engine version + save format, and the `verify` self-consistency checks still
hold. If preserving today's LOD-off hashes turns out to matter, the
alternative (hash the policy only when `enabled`) is a clean fallback, but it
is not recommended as the default.

## `genesis verify` grows an LOD mode

Add a `--lod <policy.ron>` (or a policy embedded in `--config`) path to the
`verify` subcommand. With a policy whose ladder actually demotes quiet chunks,
run the existing four-way check — two fresh runs, save/resume, single-thread —
and require all four final hashes equal. This proves LOD-on is **self-identical
across thread counts and save/resume** (invariant 3). Note in the output that
LOD-on and LOD-off are *different universes* (different hashes) by design.

## The ~10M baseline (gate before any environment feature)

Before the first environment feature lands (Q-2026-07-06-A, item 3), extend
`bench` to ~10M particles and record, in BASELINES.md: throughput LOD-off vs
LOD-on at a representative "mostly quiet" state, the active-fraction, and the
speedup. This is the number that justifies LOD existing; it belongs in the
tree before Phase 4 features build on it.

## Suggested landing order (each a self-contained, testable commit)

1. `GridGeom` chunk indexing: `chunk_of(cell)`, `chunk_count()`, derived from
   `chunk_cells`. Pure geometry, unit-tested like `neighbors_of`. No consumer
   yet, no replay-identity impact.
2. `LodPolicy` in genesis-config: struct, validation, serde default
   (`enabled: false`), RON round-trip test.
3. Classification pass producing the per-particle `active` mask (behind
   `enabled`; a no-op all-active mask when disabled).
4. Wire the both-active gate into `forces`, `interact::apply`, `integrate`.
   With `enabled: false` the mask is all-true and every hash is unchanged from
   step 2 — prove it with a hash-equality test.
5. Hash the policy + save format v8 (the one intentional hash change).
6. `verify --lod` mode + tests: LOD-on self-identical across threads/resume;
   cross-rate matter/energy stock conservation under an emit/absorb pack.
7. 10M `bench` LOD-on/off → BASELINES.md.

Steps 1–4 are behavior- and hash-preserving while `enabled: false`, so they can
land incrementally without disturbing existing replays; step 5 is the single
deliberate replay-identity change.

## Why not the alternatives

- **Catch-up integration (a cold chunk takes a `rate·dt` step when it runs)**
  keeps wall-time consistent instead of slow-motioning cold regions, but a
  boundary pair whose two chunks have different rates has no single well-defined
  `dt`, which muddies the clean both-active momentum bookkeeping above. It is a
  reasonable *later* refinement layered on the freeze groundwork, not the first
  cut. (Note: this is LOD, distinct from time-warp — the "dt never changes"
  rule is about time-warp; approximate LOD changing an effective per-chunk step
  is permitted. Even so, freeze-first is simpler and exactly conservative.)
- **One-sided force skipping (hot `i` feels frozen `j`, but `j` doesn't feel
  `i`)** would let a hot region keep integrating against a stale neighbor, but
  it breaks Newton's third law across the boundary → momentum leak. Rejected:
  it violates the hard conservation invariant.
- **Per-tick recomputation of the full force pass with a cheaper kernel for
  cold chunks** (LOD by force fidelity, not by frequency) still pays the O(n·
  neighbors) neighbor walk for every particle every tick — it does not buy the
  population-scale saving Q-2026-07-06-A is about, which is skipping the walk
  entirely for cold particles.
- **Persistent chunk objects carrying their own rate/phase state** would need
  saving (format churn) and a migration story, and reintroduce a history
  dependence the stateless-per-tick mask avoids entirely.
