# PLAYBOOK — how good work gets done here

Craft standards for any session working this repo, human or model. GOAL.md
says *what* to do; this says *how well*. Every rule below was earned in a
real session — the precedent commits are cited so you can read the diff
instead of guessing what the rule means.

## 1. Replay identity: the one invariant that rules them all

The reasoning rule, applied before any new parameter/feature ships:

> **Two configurations that produce byte-identical simulations must produce
> identical hashes. Two that can diverge must hash differently.**

Concretely: new knobs enter the state hash **only when they change the
universe** — disabled LOD contributes nothing (v8), zero env fields nothing
(v9), an empty action queue nothing (v11), zero spin nothing (v15,
`06417bb`). The save container still writes them **unconditionally** (self-
describing files), each conditional hash block gets a distinct leading tag,
and old runs keep their exact hashes with zero churn — that last property is
a correctness requirement, not a nicety.

**New-physics-param checklist** (the v15 spin commit `06417bb` is the worked
example — read it): config field + serde default + validation → snapshot
field → conditional hash block with fresh tag → persist write/read + format
version bump + layout doc comment → `from_snapshot` wiring → tests: value
survives the format, zero/disabled keeps the pre-feature hash, non-zero is a
different universe from tick 0.

## 2. Decisions: research → doc → log → code, in that order

- **Emergence-critical or physics-touching fork?** Check the literature
  before choosing. The rotation fork (Q-2026-07-10-B) was parked on a
  momentum-conservation objection that *dissolved* under research: Coriolis
  does no work (energy exact), total momentum rotates at constant magnitude
  (|P| still an invariant), and the f-plane is the standard way rotation
  enters flat 2D domains. The objection was a reason to read, not a reason
  to park. Findings live in docs/research/rotation.md with sources; the
  decisions-log entry cites it.
- **Precedent first.** Before inventing a pattern, search the decisions log
  for one: labels-above-the-engine (asteroids), only-when-enabled hashing
  (LOD), one-representation actions (Q-2026-07-08-B), sidecar bookkeeping
  (branch records). Most "new" questions are a settled precedent applied
  again — say which one in the log entry.
- **Divergence is legal but must be written.** The v8 hash decision
  deliberately contradicted its own draft doc's recommendation and says so,
  with rationale, in the decisions log. Never silently drift from a design
  doc.

## 3. Numerics: prefer the structure-preserving form

An explicit Euler term for a rotation injects energy every tick; an exact
velocity rotation (the plasma Boris-pusher trick) preserves speed bit-for-
bit and is unconditionally stable (physics.rs `integrate`, `06417bb`).
General form of the rule: when a continuous operator has a conserved
quantity, pick the discretization that conserves it *exactly* rather than
approximately — then test the invariant, not a tolerance. Same family:
id-order weight sums in impacts (`241f483`) so parallel reduction order can
never enter replay identity, and the both-active LOD gate so conservation
holds exactly at every rate.

## 4. The testing bar for sim changes

Unit tests are the floor. A sim change ships with, as applicable:

- determinism: two fresh runs, save/resume **taken mid-effect** (pending
  action in queue, spin active, LOD mid-stride), thread-count invariance;
- conservation: exact for matter (bit-comparable f64 sums over id-sorted
  snapshots), invariant-based for energy/momentum (|P| under spin, not
  componentwise);
- actions: scripted vs live-queued bit-identity (one representation);
- persistence: non-default values survive the binary format (the stored
  state hash turns a dropped field into a load error — lean on that).

Then the gate GOAL.md names: release `genesis verify` four-way on a real
shipped config/script — the full-stack pairing exists precisely to run
every feature at once (`86684bb`); extend it when you add a verb (spin was
added to scripts/full-stack.ron the same day it shipped).

## 5. Cloud/autonomous discipline

- **Stop at the testable boundary.** The app shell waited for a desktop
  session because a cloud box can't verify a window; the headless halves
  (extraction, raster, pacer, brush, inspector) all landed first. Splitting
  a feature at that boundary is a design skill — practice it.
- **Pushed units.** The next shift sees only what's on origin/main. Fetch
  before starting local-style work; a duplicate implementation of an
  already-landed item is pure waste (it has happened — check first).
- **Baselines record machine, tick count, command.** A hash without its
  tick count is unusable — that lesson cost a session an hour and is now a
  note in BASELINES.md. Don't re-run hours-long bench regimes (the 186–744x
  pack rows) without a written reason.
- **Never invent architecture to stay busy** (GOAL.md rule; it bears
  repeating because idle sessions drift toward speculative scaffolding —
  magnetic field stayed unbuilt for exactly this reason, Q-2026-07-10-D).

## 6. Writing things down

- Decisions-log entries carry the *why*, the precedent they applied, and
  what they deliberately rejected — dense enough that a fresh session can
  reconstruct the reasoning without the conversation that produced it.
- README/ROADMAP move in the same commit as the feature, not a follow-up.
- Commit messages: conventional format, subject states the change, body
  states the reasoning and the verification actually performed. If tests
  didn't run, the message must not imply they did.

## 7. Session hygiene

- Background processes you start are yours to stop; audit before ending a
  work block.
- On WSL boxes: `PATH` includes `~/.cargo/bin`,
  `CARGO_TARGET_DIR=$HOME/.cache/genesis-target` (drvfs is slow).
- End of shift = GOAL.md wind-down: tests, clippy, fmt, verify if sim
  changed, docs current, everything pushed. An unpushed session is a failed
  session.
