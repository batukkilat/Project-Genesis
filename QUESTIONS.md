# QUESTIONS — decisions needed from the project owner

Design forks not settled in the ROADMAP decisions log. Work that depends on
an entry here is blocked until the answer is recorded in the decisions log;
everything else continues. Format: context, options, recommendation.

## Q-2026-07-15-A — the Phase 6.5 exit-criterion scalar structurally favors condensation

**Context.** The exit criterion asks for a discovered regime that beats
every shipped pack on the headline scalar (max persistence ×
complexity), while the ratified fitness v1 (Q-2026-07-13-C)
deliberately *discounts* condensation. The headline-anatomy analysis
(docs/research/sweeps/2026-07-15-headline-anatomy.md) shows these now
pull in opposite directions, with measured bounds rather than
suspicion: two of complexity's three terms are priced in mean bond
degree, so no structure respecting the search's own condensation mark
(mean degree ≤ 50) can beat `actual`'s 3 631 at any population a corpus
run has actually reached — even granting a maximum-entropy degree
distribution and unbroken persistence, the ceiling is 3 598–3 614; at
*observed* degree entropy the requirement becomes a single connected
structure of ≥ 43 000 particles, four times any population ever
simulated. The corpus 20 k leaderboard is a condensation ladder
(everything above headline ~2 400 has crossed the mark), and the
search-02 champion itself crosses it between the 3 k screen and 20 k.
The criterion as written is therefore reachable only by
out-condensing `actual` — the regime class the phase's own fitness was
designed to reject.

**Options.**
(1) Keep the criterion literally; give the search a second,
condensation-tolerant fitness lane so it can honestly chase the raw
headline. Preserves the letter of the criterion; abandons the ratified
fitness philosophy and likely crowns a bigger blob.
(2) Score the exit criterion on a **bounded-degree headline**: the same
max persistence × complexity, taken only over (structure, sample) rows
with mean degree at or below the condensation mark. Report it alongside
the raw scalar in every RunScore (additive field, serde-defaulted so
committed records still load); the raw scalar keeps its meaning and
history. Aligns the criterion with what the phase actually hunts —
structured persistence rather than welding — at the cost of amending an
owner-approved success definition.
(3) Keep both scalar and criterion; reinterpret "beat every shipped
pack" as within-class (marked vs unmarked), i.e. a discovered unmarked
regime need only beat unmarked packs (current bar: full-stack's 1 806 —
the champion already beats it). Cheapest, but quietly weakens the
criterion without making the measurement better.

**Recommendation.** 2 — it is the v8/LOD reasoning applied to
measurement (two regimes the phase judges differently should be scored
by a quantity that can tell them apart), it keeps every committed
record valid, and it turns finding 4's "corner of configuration space"
into the actual target. Parked rather than adopted: the exit criterion
is the phase's owner-approved definition of success (2026-07-12), and
the 2026-07-14 shift log explicitly reserved this for an owner-level
look once evidence accumulated — it now has.

**Update 2026-07-17 (measurement completed 2026-07-20).** The
*measurement half* of option 2 shipped report-only (decisions log
Q-2026-07-17-B): every new `RunScore` carries
`persistence_complexity_bounded`, and the affordable 20 k subset
(actual, sandbox, full-stack, chains, both discovered champions) plus
the 3 k gate were re-scored to populate it — measured numbers and the
records themselves in docs/research/sweeps/2026-07-17-bounded-headline.md
(all committed state hashes reproduced bit-for-bit). Headline numbers:
`actual`, the raw bar at 3631.4, keeps only **474.0** when condensed
rows are excluded (−87 %); the measured bounded bar becomes sandbox's
**2178.5**, with the search-02 champion **3.3 % below it** (2106.6)
and the search-04 champion at 1894.2 — i.e. under option 2 the exit
criterion goes from provably-unreachable-honestly to contestable by
the already-discovered regimes. bands and sieve stay unmeasured at
20 k (cost); bands' row is the one number worth buying if option 2 is
adopted. The decision itself stays parked: the exit criterion is
still scored on the raw scalar; what changed is that choosing
option 2 (or rejecting it) can now be done against a real bounded
leaderboard instead of the anatomy doc's analytic ceilings.

## Q-2026-07-10-D — magnetic field: blocked on a radiation quantity

**Context.** The constitutional verb list includes "magnetic field", and
its only physical role (shielding radiation) references a *radiation*
environment that does not exist. Radiation itself is a fork: an env
field like any other (declare `radiation` as field N, rules gate on it —
possible today with zero engine change) vs. a particle-flux mechanism
(directional, transports energy, interacts).

**Options.** (1) magnetic field = a declared env field modulating
another declared env field via a new generic field-coupling operator;
(2) magnetic field = parameter of a future radiation-flux system;
(3) drop the verb until radiation exists (it is meaningless alone).

**Recommendation.** 3 — do nothing yet. Any mechanism invented now would
be speculative architecture (GOAL forbids inventing architecture to stay
busy). Revisit when radiation content exists. **Parked.**

**Standing guidance from the owner (2026-07-06):** when a parked question
carries a clear recommendation, autonomous sessions may adopt the
recommended option themselves — record it in the ROADMAP decisions log
(with the Q id) in the same commit, and only park here when no option is
clearly recommendable or the fork touches something irreversible.

## Resolved

- **Q-2026-07-10-B** (planet rotation on a 2D torus) → option C staged,
  **decided by the owner 2026-07-12** after a requested literature review
  (docs/research/rotation.md): frame spin (option A) now — the geophysics
  f-plane precedent; Coriolis does no work so energy conservation is
  exact, and total momentum rotates at constant magnitude rather than
  being lost (|P| stays an invariant). Applied as an exact velocity
  rotation (Boris-pusher precedent), `spin` hashes only when non-zero,
  save format v15, player verb `SpinSet`. Insolation cycling (option B)
  stays a future field-dynamics oscillator, not a rotation feature.
  Recorded in the decisions log 2026-07-12.
- **Q-2026-07-06-A** (adaptive detail exactness) → Option A, recorded in
  the decisions log 2026-07-06. Phase 4 unblocked.
- **Q-2026-07-06-B** (information overflow) → Option A, information-only
  cap `information_max` in replay identity, recorded in the decisions log
  2026-07-06. **Implemented 2026-07-06** (save format v7): clamp at
  interaction commit, cap in replay identity, verified deterministic.
- **Q-2026-07-09-B** (renderer snapshot mechanism, open design question 4)
  → lockstep-with-extraction-seam for v1 (the Observer precedent; a
  dedicated sim thread + double buffer stays a consumer-invisible upgrade
  behind the `RenderFrame` seam). Recorded in the decisions log
  2026-07-09; plan in docs/research/render-bootstrap.md. Adopted
  autonomously per the standing guidance — one option was clearly
  recommendable and nothing is irreversible (the seam is the hedge).
- **Q-2026-07-10-A** (timeline branch representation) → RON sidecar record
  above the engine (parent save path + state hash + fork tick, per-branch
  action log); binary save format untouched, ancestry never replay
  identity. Adopted autonomously per the standing guidance — two settled
  precedents (labels above the engine; observer bookkeeping outside
  identity) point the same way, and a sidecar is fully reversible, while
  embedding ancestry in the .gens container would bump the format for pure
  bookkeeping. Recorded in the decisions log 2026-07-10.
- **Q-2026-07-09-A** (asteroid impact semantics) → adopted autonomously per
  the standing guidance: the 2026-07-06 decisions-log entry already fixed
  the shape (replay-recorded event; momentum + energy shock; payload as
  quantity ranges); the remaining forks (falloff form, deposit
  normalization, payload stream derivation) had one clearly recommendable
  option each. Recorded in the decisions log 2026-07-09; save format v13.
