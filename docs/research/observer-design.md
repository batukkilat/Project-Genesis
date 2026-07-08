# Observer — Phase 5 design draft

Status: draft plan, 2026-07-08. Forks below carry recommendations; they
are ratified into the ROADMAP decisions log when the implementation
lands (the adaptive-detail precedent). Phase 5 may begin: Phase 4's exit
criteria pass.

Constitution constraints: the Observer exists outside the simulation and
never changes it; it produces hypotheses, never truth; no biological or
civilizational terms below the Observer layer — the Observer layer is
exactly where such labels become permitted, always prefixed "possible".

## What already exists

`genesis-headless/src/analysis.rs` is proto-Observer code living in the
CLI: bond-graph connected components, per-sample stats, and a
`StructureTracker` that follows components across samples by member
overlap (half-of-the-larger rule, deterministic tie-breaking).
`genesis run --report N` prints it. Phase 5 starts by promoting this
into a real crate. **Step 1 landed 2026-07-08**: the module is now the
`genesis-observer` crate, with the on/off replay-compatibility test.
**Step 2 (F3) landed 2026-07-08**: `ObserverConfig` (RON, validated,
explicitly not replay identity) carries the overlap threshold and
persistence age; tracked structures carry stable observer ids (start at
1, never reused); `genesis run --observer <path>` wires it up.

## F1 — Crate boundary and the read-only guarantee

New crate `genesis-observer` (layer 5), depending on `genesis-sim` only
for the `WorldSnapshot` type. The Observer consumes owned snapshots —
it *cannot* mutate simulation state because it never sees `&mut`
anything; replay compatibility (identical hashes with Observer on/off)
is then a type-system fact, verified by one test rather than defended
by discipline. The headless CLI's `--report` becomes a thin printer
over observer output; `analysis.rs` moves/dissolves into the crate.

## F2 — Snapshot cadence, not snapshot machinery

Open design question 4 (double-buffering vs copy-on-write for stall-free
reads) is a Phase 6 renderer problem. Phase 5 observes on its own
cadence — every N ticks, headless — via the existing `snapshot()` clone.
At 10M particles a clone per sample is measurable but the cadence is
coarse (hundreds/thousands of ticks); do not build zero-copy machinery
for a consumer that samples this rarely.

## F3 — Structure identity: configurable overlap, stable observer ids

The tracker already matches by overlap (`shared ≥ half of the larger`,
hardcoded). Generalize: the threshold becomes observer config, and each
tracked structure gains a stable observer-side id (never particle ids;
never reused) so metrics and the timeline can reference "structure 17"
across its whole life. Matching stays deterministic exactly as today
(most shared members, then canonical order, each predecessor claimed
once).

## F4 — Metrics v1 (formal definitions for the spec's names)

Per tracked structure, per sample:

- **persistence** — samples survived since first seen (age).
- **stability** — 1 − churn, churn = `|C Δ C'| / |C ∪ C'|` between
  consecutive samples (0 = frozen membership, 1 = total turnover).
  *Landed 2026-07-08*: algebraically this is the Jaccard similarity
  `|C ∩ C'| / |C ∪ C'|`, computed from the shared-member count the
  matcher already has; a newborn structure is 1.0 by convention.
- **complexity** — size plus bond-degree entropy (a chain, a ring, and a
  blob of equal size must rank differently); exact formula is an
  implementer fork. *Fork settled on landing (2026-07-08,
  Q-2026-07-08-D)*: `ln(size) + H(degree histogram, nats) +
  ln(1 + mean_degree)`. The literal "size + degree entropy" fails this
  section's own requirement — a ring and an equal-size dense blob both
  have zero degree entropy and would tie; the connectivity term
  separates them (ring < chain < blob for size 6, pinned by test).
- **information retention** — total information held by members,
  tracked across the structure's lifetime (does it hold signal, or
  leak it). *Landed 2026-07-08*: the per-sample metric is the current
  member total; lifetime trend belongs to the timeline (F6).
- *adaptation* is **deferred**: it needs environment-correlation history
  (did membership shift track a field change?) — meaningful only after
  timeline recording exists.

All thresholds and weights live in an observer config (RON), which is
NOT part of replay identity — the Observer cannot affect the simulation,
so two runs differing only in observer config are the same universe by
construction.

## F5 — Hypotheses v1: two honest ones, not five aspirational ones

A hypothesis is a confidence-scored predicate over metrics, evaluated
deterministically. v1 ships exactly two:

- **"possibly self-maintaining"** — persistence above a threshold while
  stability stays above a threshold (structure outlives its members'
  tenure).
- **"possibly growing"** — monotonic size trend over a window.

*Landed 2026-07-08 (Q-2026-07-08-E)*, exact v1 formulas (all thresholds
in `ObserverConfig`; only positive findings are recorded — absence
means "nothing to report", never "refuted"):

- self-maintaining: requires `persistence >= self_maintaining_age`
  (default 10 samples) and per-sample stability `>=
  self_maintaining_stability` (default 0.75) across the whole `window`
  (default 5). Confidence = `min(1, persistence /
  (2·self_maintaining_age)) · min(window stabilities)` — an age ramp
  capped by the worst observed churn.
- growing: requires presence in all `window` most recent samples,
  non-decreasing size, and a net increase. Confidence =
  strictly-increasing steps / (window − 1), so plateaus dilute it.

Life / intelligence / civilization / awareness labels wait for metrics
that could honestly move their confidence (self-*replication* needs
structure-signature similarity search; awareness needs player-action
correlation). Shipping them as renamed size checks would be theater.

## F6 — History recording

Per run, the observer appends sample records (tick, structures, metrics,
hypotheses) to an in-memory timeline, dumpable as RON/JSON for the
Phase 7 narrator. No save-format involvement — observer output is not
simulation state.

*Landed 2026-07-08*: `Timeline` records `TimelineSample { tick, stats,
structures, hypotheses }` per observation; `Timeline::to_ron()` is the
dump; `genesis run --report N --timeline <path>` writes it. Member
lists are deliberately NOT in the timeline (metrics reference stable
observer ids) — the narrator needs the story, not the roster.

## Landing order

1. Crate scaffold + snapshot intake + component extraction (port from
   analysis.rs) + the on/off replay-compatibility test.
2. Overlap tracker (F3) with deterministic tie-breaking + tests on
   authored bond histories.
3. Metrics v1 (F4) + observer config + tests with hand-built snapshots.
4. Hypotheses v1 (F5) + timeline dump (F6); `genesis run --report`
   switches to the crate; exit-criteria review against chains.ron /
   bands.ron runs (does the observer flag what we see by eye?).
   **Landed 2026-07-08; review passed** —
   docs/research/phase5-exit-review.md. All four steps complete.
