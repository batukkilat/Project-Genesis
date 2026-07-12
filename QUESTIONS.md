# QUESTIONS — decisions needed from the project owner

Design forks not settled in the ROADMAP decisions log. Work that depends on
an entry here is blocked until the answer is recorded in the decisions log;
everything else continues. Format: context, options, recommendation.

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
