# QUESTIONS — decisions needed from the project owner

Design forks not settled in the ROADMAP decisions log. Work that depends on
an entry here is blocked until the answer is recorded in the decisions log;
everything else continues. Format: context, options, recommendation.

## Q-2026-07-06-B: Quantity overflow policy — amplifying content can drive information to NaN

**Status:** OPEN — does not block anything current; came out of the Phase 3
exit review (docs/research/phase3-exit-review.md).

**Context.** Quantities are unbounded f32. In the review runs,
`packs/sandbox.ron` (deliberately amplifying) drove the information total
from ~4·10³ to ~4·10⁸ within 750 ticks and to **NaN** by tick 2000 —
individual values overflow to `+inf`, and the first `inf − inf` in a
transfer produces NaN, which then spreads by copy. Matter
and energy are unaffected, and NaN comparisons simply stop info-conditioned
rules from firing. Determinism is verified pre-transition (600-tick
verify); a verify run crossing the NaN tick is recorded in the review doc.
So nothing crashes — but a first-class quantity
silently becomes meaningless, which sits badly with Sandbox mode's own rule
("internal consistency required") and will confuse every Observer metric
built on information density.

**Option A — engine cap.** Clamp each quantity into `[0, quantity_max]` at
commit; `quantity_max` becomes a physics param, part of replay identity
(save format bump). Amplifying packs saturate at the cap instead of
detonating. Cost: one more parameter to explain; changes the meaning of
existing runaway packs.
- Variant: cap only `information` (matter/energy are conserved and cannot
  run away by construction).

**Option B — do nothing; content's job.** Document that amplifying packs
must bound their own economies. Keeps the engine minimal; leaves a footgun
that the project's own starter pack currently steps on.

**Option C — load-time lint.** Static warning when a pack's info actions
can net-create without a compensating sink. No replay-identity change, but
static analysis can't actually decide runaway vs bounded (it depends on
dynamics), so it's advisory at best.

**Recommendation: Option A, information-only variant.** Information is
explicitly "creatable by paying energy" (decisions log 2026-07-05), so it
is the only quantity that can overflow by design; a saturating cap in
replay identity keeps Sandbox universes internally consistent without
touching conservation semantics. Suggested default: 1e30 (far above any
meaningful signal, far below f32 overflow in transfer arithmetic).

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
