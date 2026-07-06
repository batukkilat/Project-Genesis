# QUESTIONS — decisions needed from the project owner

Design forks not settled in the ROADMAP decisions log. Work that depends on
an entry here is blocked until the answer is recorded in the decisions log;
everything else continues. Format: context, options, recommendation.

## Q-2026-07-06-A: Adaptive detail — exactness contract for reduced-rate chunks

**Status:** OPEN — blocks the first Phase 4 work item (and, by the settled
Phase 4 ordering decision, everything behind it).

**Context.** The decisions log fixes *ordering* (adaptive-detail groundwork
is the first Phase 4 item, validated with a ~10M-particle baseline) and
spec/Performance.md fixes one constraint (adaptive detail is "state-keyed,
deterministic — never wall-clock-keyed"). Neither settles the core
contract: **does enabling adaptive detail change simulation results?** This
is a replay-identity question, so it cannot be decided by the night shift.

**Option A — Approximate LOD, detail policy part of replay identity.**
Chunks are classified by a state-keyed activity metric (e.g. max particle
speed, recent interaction-event count in the chunk); cold chunks run
physics/interactions every k-th tick. The policy (metric, thresholds, rate
ladder) is config, hashed into replay identity exactly like physics params:
same seed + config + policy → bit-identical replay; a different policy is a
different universe.
- Pros: real wins at 10M+ (quiet regions get cheap); the standard approach;
  consistent with the constitution, which promises determinism for a given
  *configuration*, not resolution-independence.
- Cons: LOD-on and LOD-off runs of the same seed are different universes;
  cross-rate chunk boundaries need careful two-phase handling to stay
  order-independent and exactly conservative.

**Option B — Exact no-op skipping only.** A chunk may skip work only when
skipping is provably bit-identical (e.g. every particle at rest below the
denormal floor, no active neighbors, no rule able to fire given quantity
bounds). Replay identity untouched; LOD is purely an optimization.
- Pros: zero fidelity questions; nothing enters replay identity.
- Cons: in a kernel-force world almost no chunk is ever *provably* idle
  (forces act at range, velocities decay asymptotically), so measured wins
  are likely ~zero and the ~10M validation goal is unreachable this way.

**Option C — B now, A later.** Ship exact skipping as "groundwork", defer
approximate LOD until it is unavoidable.
- Cons: pays the boundary/machinery cost without the win; ends up
  maintaining two mechanisms.

**Recommendation: Option A**, with three hard invariants written into the
decisions log alongside it:
1. Detail policy is *configuration* — never derived from machine load,
   thread count, or wall clock (Performance.md already demands this).
2. Matter/energy conservation holds exactly at every rate, including across
   cross-rate chunk boundaries.
3. `genesis verify` grows an LOD mode proving LOD-on runs are
   self-identical across thread counts and save/resume, and the ~10M
   baseline lands in BASELINES.md before any environment feature starts.

Rationale: the constitution's rule 6 promises identical replay for the same
version + seed + configuration + actions; it does not promise that two
different configurations agree. Option A keeps that promise verbatim and is
the only option that can plausibly pass the settled "validated with a
~10M-particle baseline" gate.
