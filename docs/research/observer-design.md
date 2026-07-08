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
`StructureTracker` that ages components across samples (exact
id-set match). `genesis run --report N` prints it. Phase 5 starts by
promoting this into a real crate, then generalizing.

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

## F3 — Structure identity: overlap tracking, not exact match

The current tracker loses a structure's identity the moment one particle
joins or leaves. Generalize to id-set overlap: sample `t+1`'s component
C' continues sample `t`'s component C when
`|C ∩ C'| / max(|C|, |C'|)` ≥ a configurable threshold (default ~0.6),
best match wins, ties broken by lowest particle id (deterministic).
This yields per-structure lifetimes that survive churn — the raw
material for every metric below. Structures keep stable observer-side
ids (never particle ids; never reused).

## F4 — Metrics v1 (formal definitions for the spec's names)

Per tracked structure, per sample:

- **persistence** — samples survived since first seen (age).
- **stability** — 1 − churn, churn = `|C Δ C'| / |C ∪ C'|` between
  consecutive samples (0 = frozen membership, 1 = total turnover).
- **complexity** — size plus bond-degree entropy (a chain, a ring, and a
  blob of equal size must rank differently); exact formula is an
  implementer fork.
- **information retention** — total information held by members,
  tracked across the structure's lifetime (does it hold signal, or
  leak it).
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

Life / intelligence / civilization / awareness labels wait for metrics
that could honestly move their confidence (self-*replication* needs
structure-signature similarity search; awareness needs player-action
correlation). Shipping them as renamed size checks would be theater.

## F6 — History recording

Per run, the observer appends sample records (tick, structures, metrics,
hypotheses) to an in-memory timeline, dumpable as RON/JSON for the
Phase 7 narrator. No save-format involvement — observer output is not
simulation state.

## Landing order

1. Crate scaffold + snapshot intake + component extraction (port from
   analysis.rs) + the on/off replay-compatibility test.
2. Overlap tracker (F3) with deterministic tie-breaking + tests on
   authored bond histories.
3. Metrics v1 (F4) + observer config + tests with hand-built snapshots.
4. Hypotheses v1 (F5) + timeline dump (F6); `genesis run --report`
   switches to the crate; exit-criteria review against chains.ron /
   bands.ron runs (does the observer flag what we see by eye?).
