# Player actions — design and landing plan (Phase 4)

Status: forks settled in the ROADMAP decisions log (Q-2026-07-08-B);
implementation lands in the staged order below.

The roadmap deliverables this covers: *player environment tools (the only
player verbs)* and *player actions recorded in the replay stream*. The
phase exit criterion is the target: **a player script (headless) can shape
an environment and replay it identically.**

Constitution constraints (rule 4, rule 6): the player modifies
environments only, never organisms; same version + seed + config + player
actions = identical simulation.

## Forks and decisions

### F1 — Actions are data: a tick-stamped script, not an API

A player action is a plain data record `(tick, action)`; a headless
*action script* is a RON list of them. The UI (Phase 6) will generate
exactly the same records interactively — one representation for scripted,
recorded, and live play, so replays and saves never distinguish "who"
acted. No scripting language, no callbacks — same posture as rule packs
(data, not code).

### F2 — v1 vocabulary: environment field edits only

The only environment state that exists today is env fields, so the only
player verbs today are field edits:

- `FieldSet { field, region, value }` — set every env cell in the region.
- `FieldAdd { field, region, delta }` — add to every env cell in the region.

`region` is an axis-aligned rect in world coordinates (`x0 ≤ x < x1`,
`y0 ≤ y < y1`, clamped to the world); an env cell is affected iff its
center falls inside. A region wrapping the torus seam is authored as two
rects — no wrap machinery in v1. Values validated finite at load; field
indices validated against the config at assembly, like rule `env_cond`.

Rotation, magnetic field, tectonics, asteroid impacts arrive with the
systems they act on (asteroids already have a settled shape in the
decisions log, 2026-07-06); each extends the enum as its own deliberate
amendment. **Time warp is deliberately NOT an action**: it changes ticks
per wall second, never `dt`, so it cannot affect state and must never
enter replay identity (constitution, Gameplay spec).

### F3 — Scheduling: start-of-tick, stable script order

An action stamped `tick: T` applies at the very start of tick `T`, before
canonicalize — tick `T` simulates in the edited world. Two actions on the
same tick apply in script order (stable). Actions stamped in the past
(before the current tick at load/resume) are rejected at assembly rather
than silently dropped — a script that cannot replay identically must not
run at all. The apply step is single-threaded and touches only env cells;
thread count cannot enter.

### F4 — Replay identity: pending actions hash; applied actions are state

The state hash covers "everything that affects future ticks", so:

- **Applied actions need no separate hashing** — their entire effect is
  the env cell values they wrote, which are already hashed (v9). An
  action that sets a cell to the value it already had produces a
  byte-identical simulation and therefore the identical hash: consistent
  with the v8/v9 rule that identical universes hash identically.
- **Pending actions (tick still in the future) DO hash** — two runs with
  identical current state but different pending edits have different
  futures, and the hash must say so (same reason the rule set hashes).
  An empty pending queue contributes nothing, so every action-free run
  keeps its exact identity (v8/v9 precedent).
- Save format v11 stores the pending queue unconditionally (possibly
  count 0), so a mid-script save resumes into the identical future.
  Consumed actions are dropped from the queue as their tick passes.

### F5 — CLI: `--actions script.ron` on run, bench, and verify

`genesis run --config configs/env-gradient.ron --rules packs/bands.ron
--actions scripts/flip.ron` runs a scripted experiment headless.
`genesis verify --actions ...` proves the scripted run deterministic
(fresh/fresh, save/resume mid-script, thread counts) — that command *is*
the phase exit criterion, executable.

## Landing order

1. **Script format + validation** in genesis-config (`ActionScript`,
   `PlayerAction`, `RegionSpec`), RON load, finite/ordering checks.
2. **Apply + scheduling** in genesis-sim: pending queue resource sorted
   by (tick, script index); start-of-tick drain applies matching actions
   to `EnvFields`; assembly rejects past-stamped actions and bad field
   indices.
3. **Replay identity + save v11**: pending queue in snapshot, hashed only
   when non-empty; persisted unconditionally; resume mid-script tested
   against uninterrupted.
4. **CLI + exit-criterion content**: `--actions` flag, a shipped example
   script (e.g. moving the `bands.ron` band mid-run), and a test that the
   scripted edit visibly redirects where structures form afterward.

## Notes

- An action edits *state*, so undo is a new action, not a rollback —
  histories only move forward (constitution: "there are no failures —
  only histories"; branching is a Phase 6 save-fork concern).
- Field dynamics (diffusion/decay/sources) remain a separate later item;
  when they land, player edits compose with them naturally (an edit is
  just different cell values for the operators to evolve).
- Performance: the drain is O(pending this tick) with an O(cells in
  region) write — zero cost on ticks with no actions.
