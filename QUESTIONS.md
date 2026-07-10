# QUESTIONS — decisions needed from the project owner

Design forks not settled in the ROADMAP decisions log. Work that depends on
an entry here is blocked until the answer is recorded in the decisions log;
everything else continues. Format: context, options, recommendation.

## Q-2026-07-10-B — planet rotation: what is it in a 2D torus?

**Context.** The constitution lists "planet rotation" among the player's
environment verbs (rule 4); ROADMAP Phase 4 defers it until its system
lands. On a 2D torus viewed top-down there is no rotation axis, so the
verb needs a physical reading before it can be a system. Whatever is
chosen becomes a physics/replay-identity change (new param, save format
bump) and shapes what can emerge — this is an emergence-critical fork
like information semantics was.

**Options.**
- **A — frame spin (Coriolis-style).** A `spin` physics param; every
  particle gets the generic velocity-dependent acceleration
  `a = 2·spin·perp(v)`. Player action `SpinSet(value)`. Generic (no Earth
  assumption), cheap (one fused multiply in integrate), and it visibly
  bends trajectories — vortices and shear bands can emerge. But it breaks
  momentum conservation by design (a rotating frame is non-inertial), so
  conservation accounting needs a carve-out, and time-reversal symmetry
  of the kernel changes.
- **B — insolation cycling.** Rotation = a periodic driver for env
  fields: a config-declared oscillation (period in ticks, axis, phase)
  that modulates a chosen field (the "day/night" reading). Pure env-layer
  change (fields already exist; dynamics already hash conditionally); no
  physics touch; but it silently reintroduces the *source* concept cut
  from field dynamics v1, and rotation stops being felt by particles
  directly.
- **C — both, staged.** A as the Phase 4 verb (it is the mechanical
  reading of "rotation"); B later as a generic field-oscillator when
  content demands day/night — explicitly a field-dynamics extension, not
  a rotation feature.

**Recommendation (weak).** C's first half — A alone — is the honest
mechanical reading, but the momentum-conservation carve-out contradicts
"matter and energy conserved" instincts and deserves owner sign-off;
B alone reads as rotation only by convention. No option is clearly
recommendable without deciding how much non-inertial physics the
constitution's "Actual Physics" mode tolerates. **Parked.**

## Q-2026-07-10-C — tectonic events: what do they do?

**Context.** "Tectonic events" is a constitutional player verb; nothing
in the engine models solid substrate, plates, or terrain — there is no
height, no ground, only particles and env fields. The verb needs a
mechanically-honest v1.

**Options.**
- **A — line-source impact.** A recorded action like `Impact`, but the
  shock source is a world-coordinate *segment* instead of a point:
  momentum impulse perpendicular to the segment (shear/rift), energy
  deposit with distance falloff, optional particle payload (upwelling).
  Reuses the entire impact machinery (falloff weights, order-free payload
  RNG, pending-hash rules); a rift is authorable today.
- **B — env-field rewrite event.** A tectonic event edits env fields in
  a band (e.g. steps a "temperature-like" field along a line), letting
  rules react — no direct particle touch. Weaker mechanically; mostly
  achievable already with FieldSet regions.
- **C — A + B composed.** One recorded event carrying both a line shock
  and a set of band field edits.

**Recommendation.** A is nearly clearly-recommendable (pure
generalization of a shipped, settled system; conserves exactly like
impacts; no new concepts below the Observer). Parked only because the
verb is constitutional surface area and cheap to confirm — if a session
needs it before an answer arrives, A is the option to adopt per the
standing guidance.

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
